use super::recurrence_completion_tests::{dump, fixture, fixture_with_limit};
use super::*;
use crate::recurrence_protocol::RecurrenceDocument;
use crate::{recurrence_store, recurrence_transaction};

#[test]
fn delete_current_skips_once_and_replay_is_noop() {
    let (mut db, todo, _) = fixture();
    let result = soft_delete_todo_in_connection(&mut db, todo.id, None).unwrap();
    assert_eq!(result.deleted_todos.len(), 1);
    assert!(!result.deleted_todos[0].completed);
    assert!(result.deleted_todos[0].deleted_at.is_some());
    let next = result.created_todo.unwrap();
    assert_eq!(next.due_date.as_deref(), Some("2026-09-06"));
    assert_eq!(
        recurrence_store::snapshot(&db).unwrap().document.rules[0].current_todo_uuid,
        next.uuid
    );
    let before = dump(&db);
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
        .unwrap()
        .deleted_todos
        .is_empty());
    assert!(!complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    assert_eq!(dump(&db), before);
}

#[test]
fn undo_and_redelete_history_do_not_rewind_or_generate_again() {
    let (mut db, todo, _) = fixture();
    soft_delete_todo_in_connection(&mut db, todo.id, None).unwrap();
    let rules = recurrence_store::snapshot(&db).unwrap();
    let future = now_millis() + 86400000;
    db.execute(
        "UPDATE todos SET updated_at=?1 WHERE id=?2",
        params![future, todo.id],
    )
    .unwrap();
    assert_eq!(
        restore_todo_in_connection(&mut db, todo.id)
            .unwrap()
            .updated_at,
        future + 1
    );
    let before = dump(&db);
    restore_todo_in_connection(&mut db, todo.id).unwrap();
    assert_eq!(dump(&db), before);
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
        .unwrap()
        .created_todo
        .is_none());
    assert_eq!(
        recurrence_store::snapshot(&db).unwrap().revision,
        rules.revision
    );
    assert_eq!(list_todos_from_connection(&db).unwrap().len(), 1);
}

#[test]
fn deletion_of_last_instance_exhausts_and_undo_does_not_reactivate() {
    let (mut db, todo, _) = fixture_with_limit(true);
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
        .unwrap()
        .created_todo
        .is_none());
    assert!(recurrence_store::snapshot(&db).unwrap().document.rules[0].exhausted);
    restore_todo_in_connection(&mut db, todo.id).unwrap();
    assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .is_none());
}

#[test]
fn delete_series_stops_rule_atomically_and_undo_does_not_resume() {
    let (mut db, todo, old_rule) = fixture();
    let next = set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .unwrap();
    let result = soft_delete_todo_in_connection(&mut db, next.id, Some("series")).unwrap();
    assert_eq!(result.deleted_todos.len(), 2);
    assert!(result.created_todo.is_none());
    let stopped = recurrence_store::snapshot(&db).unwrap();
    assert!(stopped.document.rules[0].deleted_at.is_some());
    restore_todo_in_connection(&mut db, next.id).unwrap();
    assert!(set_todo_completed_in_connection(&mut db, next.id, true)
        .unwrap()
        .created_todo
        .is_none());
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![old_rule],
        },
    )
    .unwrap();
    assert_eq!(
        recurrence_store::snapshot(&db).unwrap().document,
        stopped.document
    );
}

#[test]
fn series_delete_before_first_binding_includes_rule_and_first_task() {
    let (mut db, todo, _) = fixture();
    assert!(todo.repeat_series_uuid.is_none());
    let deleted = soft_delete_todo_in_connection(&mut db, todo.id, Some("series")).unwrap();
    assert_eq!(deleted.deleted_todos.len(), 1);
    assert!(deleted.created_todo.is_none());
    assert!(recurrence_store::snapshot(&db).unwrap().document.rules[0]
        .deleted_at
        .is_some());
}

#[test]
fn stop_only_keeps_existing_tasks_and_is_idempotent() {
    let (mut db, todo, rule) = fixture();
    let before = find_todo(&db, todo.id).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        recurrence_transaction::stop_series_for_todo(&tx, &todo.uuid, 2000, "device-b").unwrap(),
        Some(rule.first_todo_uuid)
    );
    tx.commit().unwrap();
    assert_eq!(find_todo(&db, todo.id).unwrap(), before);
    let saved = dump(&db);
    let tx = db.transaction().unwrap();
    recurrence_transaction::stop_series_for_todo(&tx, &todo.uuid, 3000, "device-b").unwrap();
    tx.commit().unwrap();
    assert_eq!(dump(&db), saved);
}

#[test]
fn deletion_failures_roll_back_tombstone_next_rule_and_dirty() {
    for trigger in [
        "BEFORE INSERT ON todos",
        "BEFORE UPDATE ON recurrence_rules",
        "BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'recurrence.instance.v1:%'",
    ] {
        let (mut db, todo, _) = fixture();
        let before = dump(&db);
        db.execute_batch(&format!(
            "CREATE TRIGGER fail {trigger} BEGIN SELECT RAISE(ABORT,'injected'); END"
        ))
        .unwrap();
        assert!(soft_delete_todo_in_connection(&mut db, todo.id, None).is_err());
        assert_eq!(dump(&db), before);
        db.execute_batch("DROP TRIGGER fail").unwrap();
        assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
            .unwrap()
            .created_todo
            .is_some());
    }
}

#[test]
fn later_series_delete_failure_rolls_back_first_task_and_rule_stop() {
    let (mut db, todo, _) = fixture();
    let next = set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .unwrap();
    let before = dump(&db);
    db.execute_batch(&format!("CREATE TRIGGER fail BEFORE UPDATE ON todos WHEN NEW.id={} BEGIN SELECT RAISE(ABORT,'injected'); END",next.id)).unwrap();
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, Some("series")).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn invalid_scope_and_conflicting_links_leave_database_unchanged() {
    for scope in ["single", "series", "invalid"] {
        let (mut db, todo, _) = fixture();
        db.execute_batch("UPDATE todos SET repeat_rule='daily'")
            .unwrap();
        let before = dump(&db);
        assert!(soft_delete_todo_in_connection(&mut db, todo.id, Some(scope)).is_err());
        assert_eq!(dump(&db), before);
    }
}

#[test]
fn unknown_series_cannot_be_silently_stopped_but_single_delete_is_allowed() {
    let (mut db, todo, _) = fixture();
    db.execute_batch("DELETE FROM recurrence_rules").unwrap();
    db.execute("UPDATE todos SET repeat_series_uuid=uuid", [])
        .unwrap();
    let before = dump(&db);
    assert!(
        soft_delete_todo_in_connection(&mut db, todo.id, Some("series"))
            .unwrap_err()
            .contains("LINK_CONFLICT")
    );
    assert_eq!(dump(&db), before);
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
        .unwrap()
        .created_todo
        .is_none());
}

#[test]
fn skip_at_maximum_safe_clock_does_not_attempt_a_second_write() {
    let (mut db, todo, _) = fixture();
    db.execute("UPDATE todos SET updated_at=9007199254740990", [])
        .unwrap();
    let result = soft_delete_todo_in_connection(&mut db, todo.id, None).unwrap();
    assert_eq!(result.deleted_todos[0].updated_at, 9007199254740991);
    assert!(result.created_todo.is_some());
}

#[test]
fn deleting_history_preserves_current_and_archived_task_cannot_skip() {
    let (mut db, todo, _) = fixture();
    let next = set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .unwrap();
    let revision = recurrence_store::snapshot(&db).unwrap().revision;
    assert!(soft_delete_todo_in_connection(&mut db, todo.id, None)
        .unwrap()
        .created_todo
        .is_none());
    assert_eq!(recurrence_store::snapshot(&db).unwrap().revision, revision);
    db.execute("UPDATE todos SET archived_at=100 WHERE id=?1", [next.id])
        .unwrap();
    let before = dump(&db);
    assert!(soft_delete_todo_in_connection(&mut db, next.id, None)
        .unwrap()
        .deleted_todos
        .is_empty());
    assert_eq!(dump(&db), before);
}
