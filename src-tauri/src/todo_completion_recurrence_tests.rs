use super::*;
use crate::recurrence_protocol::{RecurrenceDocument, RecurrenceRule};
use crate::recurrence_store;

pub(super) fn fixture() -> (Connection, Todo, RecurrenceRule) {
    fixture_with_limit(false)
}

pub(super) fn fixture_with_limit(last: bool) -> (Connection, Todo, RecurrenceRule) {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule: RecurrenceRule = serde_json::from_value(raw["base_rule"].clone()).unwrap();
    if last {
        rule.schedule.end_type = "count".into();
        rule.schedule.max_occurrences = Some(1);
    }
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![rule.clone()],
        },
    )
    .unwrap();
    db.execute("INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,due_date,reminder_at,updated_by) VALUES(?1,'title','note',0,1,1,'2026-09-05',1000,'device-a')", [&rule.first_todo_uuid]).unwrap();
    let id = db.last_insert_rowid();
    let todo = find_todo(&db, id).unwrap().unwrap();
    (db, todo, rule)
}

pub(super) fn dump(db: &Connection) -> Vec<String> {
    [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .flat_map(|table| {
        let mut statement = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let count = statement.column_count();
        statement
            .query_map([], |row| {
                Ok((0..count)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    })
    .collect()
}

#[test]
fn normal_completion_advances_and_replays_without_changes() {
    let (mut db, todo, rule) = fixture();
    let result = set_todo_completed_in_connection(&mut db, todo.id, true).unwrap();
    assert!(result.updated_todo.completed);
    let next = result.created_todo.unwrap();
    assert_eq!(next.due_date.as_deref(), Some("2026-09-06"));
    assert_eq!(next.repeat_rule, None);
    assert_eq!(
        next.repeat_series_uuid.as_deref(),
        Some(rule.first_todo_uuid.as_str())
    );
    assert_eq!(
        recurrence_store::snapshot(&db).unwrap().document.rules[0].current_todo_uuid,
        next.uuid
    );
    let before = dump(&db);
    assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .is_none());
    assert_eq!(dump(&db), before);
}

#[test]
fn notification_receipt_survives_undo_without_rewinding_rule() {
    let (mut db, todo, _) = fixture();
    assert!(complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    let rules = recurrence_store::snapshot(&db).unwrap();
    set_todo_completed_in_connection(&mut db, todo.id, false).unwrap();
    assert!(!complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    assert!(!find_todo(&db, todo.id).unwrap().unwrap().completed);
    assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
        .unwrap()
        .created_todo
        .is_none());
    assert_eq!(
        recurrence_store::snapshot(&db).unwrap().revision,
        rules.revision
    );
    assert_eq!(list_todos_from_connection(&db).unwrap().len(), 2);
}

#[test]
fn completed_current_with_lagging_rule_reconciles_on_normal_completion() {
    let (mut db, todo, _) = fixture();
    db.execute("UPDATE todos SET completed=1,completed_at=2000", [])
        .unwrap();
    let result = set_todo_completed_in_connection(&mut db, todo.id, true).unwrap();
    assert!(result.created_todo.is_some());
    assert_eq!(result.updated_todo.completed_at, Some(2000));
}

#[test]
fn stopped_and_exhausted_rules_do_not_generate_again() {
    for stopped in [true, false] {
        let (mut db, todo, mut rule) = fixture_with_limit(!stopped);
        if stopped {
            rule.deleted_at = Some(2000);
        } else {
            rule.schedule.end_type = "count".into();
            rule.schedule.max_occurrences = Some(1);
        }
        rule.updated_at = 2000;
        recurrence_store::merge(
            &mut db,
            &RecurrenceDocument {
                format_version: 1,
                rules: vec![rule],
            },
        )
        .unwrap();
        assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
            .unwrap()
            .created_todo
            .is_none());
        set_todo_completed_in_connection(&mut db, todo.id, false).unwrap();
        assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
            .unwrap()
            .created_todo
            .is_none());
        assert_eq!(list_todos_from_connection(&db).unwrap().len(), 1);
    }
}

#[test]
fn failed_completion_rolls_back_task_next_rule_dirty_and_reminder_receipt() {
    for trigger in [
        "BEFORE INSERT ON todos",
        "BEFORE UPDATE ON recurrence_rules",
        "BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'recurrence.instance.v1:%'",
        "BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'reminder.complete.v1:%'",
    ] {
        let (mut db, todo, _) = fixture();
        let before = dump(&db);
        db.execute_batch(&format!(
            "CREATE TRIGGER fail {trigger} BEGIN SELECT RAISE(ABORT,'injected'); END"
        ))
        .unwrap();
        assert!(complete_todo_from_reminder(&mut db, &todo.uuid, 1000).is_err());
        assert_eq!(dump(&db), before);
        db.execute_batch("DROP TRIGGER fail").unwrap();
        assert!(complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    }
}

#[test]
fn old_rule_or_foreign_series_conflict_cannot_partially_complete() {
    for sql in [
        "UPDATE todos SET repeat_rule='daily'",
        "UPDATE todos SET repeat_series_uuid='foreign'",
    ] {
        let (mut db, todo, _) = fixture();
        db.execute_batch(sql).unwrap();
        let before = dump(&db);
        assert!(set_todo_completed_in_connection(&mut db, todo.id, true)
            .unwrap_err()
            .contains("LINK_CONFLICT"));
        assert_eq!(dump(&db), before);
    }
}

#[test]
fn stale_reminder_deleted_and_archived_sources_never_advance() {
    let (mut db, todo, _) = fixture();
    let before = dump(&db);
    assert!(!complete_todo_from_reminder(&mut db, &todo.uuid, 2000).unwrap());
    assert_eq!(dump(&db), before);
    for column in ["deleted_at", "archived_at"] {
        db.execute_batch(&format!("UPDATE todos SET {column}=1"))
            .unwrap();
        let before = dump(&db);
        assert!(set_todo_completed_in_connection(&mut db, todo.id, true).is_err());
        assert!(!complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
        assert_eq!(dump(&db), before);
        db.execute_batch(&format!("UPDATE todos SET {column}=NULL"))
            .unwrap();
    }
}

#[test]
fn undo_preserves_monotonic_task_clock_without_rewinding_rule() {
    let (mut db, todo, _) = fixture();
    set_todo_completed_in_connection(&mut db, todo.id, true).unwrap();
    let future = now_millis() + 86400000;
    db.execute(
        "UPDATE todos SET updated_at=?1 WHERE id=?2",
        params![future, todo.id],
    )
    .unwrap();
    let revision = recurrence_store::snapshot(&db).unwrap().revision;
    let result = set_todo_completed_in_connection(&mut db, todo.id, false).unwrap();
    assert_eq!(result.updated_todo.updated_at, future + 1);
    assert_eq!(recurrence_store::snapshot(&db).unwrap().revision, revision);
}

#[test]
fn notification_reconcile_requests_refresh_even_for_already_completed_task() {
    let (mut db, todo, _) = fixture();
    db.execute("UPDATE todos SET completed=1,completed_at=2000", [])
        .unwrap();
    assert!(complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    assert!(!complete_todo_from_reminder(&mut db, &todo.uuid, 1000).unwrap());
    assert_eq!(list_todos_from_connection(&db).unwrap().len(), 2);
}

#[test]
fn existing_next_task_is_preserved_and_deleted_next_is_not_returned() {
    for deleted in [false, true] {
        let (mut db, todo, rule) = fixture();
        let plan = crate::recurrence_progress::plan_recurrence_advance(
            &rule,
            Some(&crate::recurrence_progress::CompletionEvidence {
                uuid: todo.uuid.clone(),
                completed: true,
                deleted_at: None,
                archived_at: None,
            }),
            2000,
            "device-b",
        )
        .unwrap()
        .unwrap();
        let next = plan.next.unwrap();
        db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,repeat_series_uuid,deleted_at,updated_by) VALUES(?1,'remote title',-1024,1,9000,?2,?3,'device-z')",
            params![next.uuid, rule.first_todo_uuid, if deleted { Some(9000) } else { None }]).unwrap();
        let result = set_todo_completed_in_connection(&mut db, todo.id, true).unwrap();
        if deleted {
            assert!(result.created_todo.is_none());
        } else {
            assert_eq!(result.created_todo.unwrap().title, "remote title");
        }
        let count: i64 = db
            .query_row("SELECT count(*) FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
