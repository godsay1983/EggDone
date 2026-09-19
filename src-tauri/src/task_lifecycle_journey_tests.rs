//! Production command/repository journey on isolated SQLite, not UI or cloud acceptance.
use super::*;
use crate::{archive, daily_plan_store as plans, purge, task_workflow_store as workflow, trash};

const TASK: &str = "123e4567-e89b-42d3-a456-426614174000";
const OTHER: &str = "123e4567-e89b-42d3-a456-426614174001";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174002";

fn plan(db: &mut Connection, day: &str) {
    let request = plans::DailyPlanWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: TASK.into(),
        plan_date: day.into(),
        action: "add".into(),
        expected: plans::list(db, day).unwrap().revision,
    };
    let writer = device_id(db).unwrap();
    plans::write(db, &request, now_millis(), &writer).unwrap();
}

fn wait(db: &mut Connection, day: &str, remove: bool) {
    let request = workflow::WorkflowWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: TASK.into(),
        state: "waiting".into(),
        reason: "Private journey reason".into(),
        review_date: Some(day.into()),
        date: day.into(),
        remove_from_plan: remove,
        expected: workflow::list(db, day).unwrap().revision,
        expected_plan: if remove {
            Some(plans::list(db, day).unwrap().revision)
        } else {
            None
        },
    };
    let writer = device_id(db).unwrap();
    workflow::write(db, &request, now_millis(), &writer).unwrap();
}

fn count(db: &Connection, sql: &str, uuid: &str) -> i64 {
    db.query_row(sql, [uuid], |r| r.get(0)).unwrap()
}

fn journey(remove: bool, upgrade: bool) {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/archive-storage-v1.json")).unwrap();
    for sql in fixture["seed"].as_array().unwrap() {
        db.execute_batch(sql.as_str().unwrap()).unwrap();
    }
    if upgrade {
        crate::db::remove_task_workflow_schema_for_test(&db);
        crate::db::migrate(&mut db).unwrap();
    }
    let day = local_date_from_timestamp(now_millis()).unwrap();
    let writer = device_id(&db).unwrap();
    let initial = archive::preview(&mut db, TASK).unwrap();
    let operation = Uuid::new_v4().to_string();
    archive::apply(
        &mut db,
        &operation,
        archive::Action::Reopen,
        &initial.expected,
        now_millis(),
        &writer,
    )
    .unwrap();
    assert_eq!(
        archive::apply(
            &mut db,
            &operation,
            archive::Action::Reopen,
            &initial.expected,
            now_millis(),
            &writer
        )
        .unwrap()
        .outcome,
        "already_applied"
    );
    let id: i64 = db
        .query_row("SELECT id FROM todos WHERE uuid=?1", [TASK], |r| r.get(0))
        .unwrap();
    let reopened = find_todo(&db, id).unwrap().unwrap();
    assert!(!reopened.completed);
    assert_eq!(reopened.due_date.as_deref(), Some("2026-09-16"));
    assert!(reopened.reminder_at.is_none() && reopened.repeat_rule.is_none());

    plan(&mut db, &day);
    wait(&mut db, &day, remove);
    assert_eq!(
        find_todo(&db, id).unwrap().unwrap(),
        reopened,
        "planning/waiting must not edit task fields"
    );
    let waiting = workflow::list(&mut db, &day).unwrap();
    assert_eq!(waiting.entries.len(), 1);
    assert!(waiting.entries[0].review_due);
    assert_eq!(
        plans::list(&mut db, &day).unwrap().current.len(),
        usize::from(!remove)
    );
    let old_workflow = workflow::snapshot(&mut db).unwrap().document;
    let old_plans = plans::snapshot(&mut db).unwrap().document;
    let old_backup = crate::data_exchange::planning_test_export(&mut db).unwrap();
    set_todo_completed_in_connection(&mut db, id, true).unwrap();
    assert!(workflow::list(&mut db, &day).unwrap().entries.is_empty());
    let completed = plans::list(&mut db, &day).unwrap();
    assert_eq!(completed.current.len(), usize::from(!remove));
    if !remove {
        assert_eq!(completed.current[0].status, "completed");
    }
    assert_eq!(archive_completed_todos_in_connection(&db).unwrap(), 1);
    let archived = archive::preview(&mut db, TASK).unwrap();
    assert_eq!(archived.checklist_json, initial.checklist_json);
    assert_eq!(archived.links_json, initial.links_json);
    archive::apply(
        &mut db,
        &Uuid::new_v4().to_string(),
        archive::Action::Unarchive,
        &archived.expected,
        now_millis(),
        &writer,
    )
    .unwrap();
    assert!(find_todo(&db, id).unwrap().unwrap().completed);
    set_todo_completed_in_connection(&mut db, id, false).unwrap();
    plans::restore(&mut db, &old_plans).unwrap();
    workflow::restore(&mut db, &old_workflow).unwrap();
    assert!(workflow::list(&mut db, &day).unwrap().entries.is_empty());
    assert!(plans::list(&mut db, &day).unwrap().current.is_empty());

    soft_delete_todo_in_connection(&mut db, id, None).unwrap();
    let deleted = trash::preview(&db, trash::TrashKind::Todo, TASK).unwrap();
    trash::restore(&mut db, &deleted, now_millis(), &writer).unwrap();
    assert!(trash::restore(&mut db, &deleted, now_millis(), &writer).is_err());
    plans::restore(&mut db, &old_plans).unwrap();
    workflow::restore(&mut db, &old_workflow).unwrap();
    assert!(workflow::list(&mut db, &day).unwrap().entries.is_empty());
    assert!(plans::list(&mut db, &day).unwrap().current.is_empty());
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM task_checklist_items WHERE todo_uuid=?1 AND active=1",
            TASK
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM task_note_links WHERE todo_uuid=?1 AND active=1",
            TASK
        ),
        0
    );

    plan(&mut db, &day);
    wait(&mut db, &day, false);
    assert_eq!(
        workflow::list(&mut db, &day).unwrap().entries.len(),
        1,
        "fresh explicit waiting still works"
    );
    soft_delete_todo_in_connection(&mut db, id, None).unwrap();
    let selected = purge::prepare(
        &mut db,
        Some(vec![purge::Target {
            kind: "todo".into(),
            uuid: TASK.into(),
        }]),
        now_millis(),
    )
    .unwrap();
    let result = purge::execute_batch(&mut db, &selected.operation_uuid, now_millis()).unwrap();
    assert_eq!((result.purged, result.pending, result.skipped), (1, 0, 0));
    assert_eq!(
        purge::execute_batch(&mut db, &selected.operation_uuid, now_millis()).unwrap(),
        result
    );
    plans::restore(&mut db, &old_plans).unwrap();
    workflow::restore(&mut db, &old_workflow).unwrap();
    assert!(crate::data_exchange::planning_test_import(&mut db, &old_backup).is_err());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM todos WHERE uuid=?1", TASK),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM task_workflow_states WHERE task_uuid=?1",
            TASK
        ),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM task_checklist_items WHERE todo_uuid=?1",
            TASK
        ),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM todos WHERE uuid=?1 AND archived_at IS NOT NULL",
            OTHER
        ),
        1
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM notes WHERE uuid=?1 AND deleted_at IS NULL AND content='Keep note'", NOTE), 1);
}

#[test]
fn lifecycle_journey_waiting_keeps_plan() {
    journey(false, false);
}
#[test]
fn lifecycle_journey_waiting_removes_plan() {
    journey(true, false);
}
#[test]
fn lifecycle_journey_upgrade25_keeps_plan() {
    journey(false, true);
}
#[test]
fn lifecycle_journey_upgrade25_removes_plan() {
    journey(true, true);
}
