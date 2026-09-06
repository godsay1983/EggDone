use super::recurrence_completion_tests::fixture;
use super::*;

#[test]
fn ordinary_schedule_keeps_custom_series_and_completion_advances() {
    let (mut db, todo, rule) = fixture();
    db.execute(
        "UPDATE todos SET repeat_series_uuid=?1 WHERE id=?2",
        params![rule.first_todo_uuid, todo.id],
    )
    .unwrap();
    let result = set_todo_schedule_in_connection(
        &mut db,
        todo.id,
        Some("2026-09-07".into()),
        None,
        Some(5000),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        result.updated_todos[0].repeat_series_uuid.as_deref(),
        Some(rule.first_todo_uuid.as_str())
    );
    assert_eq!(result.updated_todos[0].reminder_at, Some(5000));
    let completed = set_todo_completed_in_connection(&mut db, todo.id, true).unwrap();
    assert!(completed.created_todo.is_some());
}

#[test]
fn legacy_picker_cannot_overwrite_known_or_stopped_custom_rule() {
    let (mut db, todo, rule) = fixture();
    let before = recurrence_completion_tests::dump(&db);
    assert!(set_todo_schedule_in_connection(
        &mut db,
        todo.id,
        Some("2026-09-05".into()),
        None,
        None,
        Some("daily".into()),
        None
    )
    .is_err());
    assert_eq!(before, recurrence_completion_tests::dump(&db));
    crate::recurrence_editor::stop_rule(&mut db, &rule, 3000, "device-b").unwrap();
    assert!(set_todo_schedule_in_connection(
        &mut db,
        todo.id,
        Some("2026-09-05".into()),
        None,
        None,
        Some("weekly".into()),
        None
    )
    .is_err());
}

#[test]
fn clear_and_archive_completed_keep_active_successor_and_rule() {
    for archive in [false, true] {
        let (mut db, todo, _) = fixture();
        let next = set_todo_completed_in_connection(&mut db, todo.id, true)
            .unwrap()
            .created_todo
            .unwrap();
        let before = crate::recurrence_store::snapshot(&db).unwrap();
        if archive {
            archive_completed_todos_in_connection(&db).unwrap();
        } else {
            clear_completed_todos_in_connection(&db).unwrap();
        }
        assert_eq!(
            crate::recurrence_store::snapshot(&db).unwrap().document,
            before.document
        );
        let next = find_todo(&db, next.id).unwrap().unwrap();
        assert!(!next.completed && next.deleted_at.is_none() && next.archived_at.is_none());
        assert!(set_todo_completed_in_connection(&mut db, next.id, true)
            .unwrap()
            .created_todo
            .is_some());
    }
}
