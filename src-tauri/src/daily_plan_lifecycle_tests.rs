//! Parent snapshots must not invent lifecycle authority before the planning sidecar arrives.
use crate::{daily_plan_protocol as protocol, daily_plan_store as plans, data_exchange, sync};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const TASK: &str = "123e4567-e89b-42d3-a456-426614174000";
const DAY: &str = "2026-09-19";

fn database() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let writer = crate::db::device_id(&db).unwrap();
    db.execute(
        "INSERT INTO todos(uuid,title,completed,sort_order,created_at,updated_at,updated_by)
         VALUES(?1,'Original title',0,0,1,1,?2)",
        params![TASK, writer],
    )
    .unwrap();
    db
}

fn add_plan(db: &mut Connection) {
    let writer = crate::db::device_id(db).unwrap();
    let request = plans::DailyPlanWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: TASK.into(),
        plan_date: DAY.into(),
        action: "add".into(),
        expected: plans::list(db, DAY).unwrap().revision,
    };
    plans::write(db, &request, 3, &writer).unwrap();
}

fn document(db: &mut Connection) -> protocol::Document {
    plans::snapshot(db).unwrap().document
}

fn merge_planning(db: &mut Connection, remote: &protocol::Document) {
    let remote = protocol::parse(&protocol::encode(remote).unwrap()).unwrap();
    let tx = db.transaction().unwrap();
    plans::merge_in_transaction(&tx, &remote).unwrap();
    tx.commit().unwrap();
}

fn merge_todos(source: &Connection, target: &mut Connection) {
    let raw = serde_json::to_string(&sync::build_document(source, 20).unwrap()).unwrap();
    let remote = serde_json::from_str(&raw).unwrap();
    sync::merge_remote_document(target, &remote, 21).unwrap();
}

fn complete_then_edit_title(db: &Connection) {
    db.execute(
        "UPDATE todos SET completed=1,completed_at=10,updated_at=10 WHERE uuid=?1",
        [TASK],
    )
    .unwrap();
    db.execute(
        "UPDATE todos SET title='Edited after completion',updated_at=11 WHERE uuid=?1",
        [TASK],
    )
    .unwrap();
}

fn event_id(version: i64, writer: &str, completed: bool) -> String {
    let hex: String = writer.bytes().map(|byte| format!("{byte:02x}")).collect();
    format!("{version}:{hex}:{}:-:-", i32::from(completed))
}

fn assert_completed_parent_at_eleven(db: &Connection) {
    let parent: (bool, i64, Option<i64>, String) = db
        .query_row(
            "SELECT completed,updated_at,completed_at,title FROM todos WHERE uuid=?1",
            [TASK],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        parent,
        (true, 11, Some(10), "Edited after completion".into())
    );
}

fn assert_completed_plan(db: &mut Connection) {
    let view = plans::list(db, DAY).unwrap();
    assert_eq!(view.current.len(), 1);
    assert_eq!(view.current[0].task_uuid, TASK);
    assert_eq!(view.current[0].status, "completed");
}

fn assert_local_reopen_still_emits_event(db: &mut Connection, completion: &str) {
    let writer = crate::db::device_id(db).unwrap();
    db.execute(
        "UPDATE todos SET completed=0,completed_at=NULL,updated_at=12,updated_by=?1 WHERE uuid=?2",
        params![writer, TASK],
    )
    .unwrap();
    let current = document(db);
    assert_eq!(current.events.len(), 2);
    assert!(current
        .events
        .iter()
        .any(|event| event.event_id == completion));
    assert!(current
        .events
        .iter()
        .any(|event| event.event_id == event_id(12, &writer, false)));
    assert!(plans::list(db, DAY).unwrap().current.is_empty());
}

#[test]
fn remote_completion_then_title_edit_preserves_the_original_completion_receipt() {
    let mut source = database();
    let mut target = database();
    add_plan(&mut source);
    merge_planning(&mut target, &document(&mut source));
    let writer = crate::db::device_id(&source).unwrap();
    complete_then_edit_title(&source);
    let authoritative = document(&mut source);
    let completion = event_id(10, &writer, true);
    assert_eq!(authoritative.events.len(), 1);
    assert_eq!(authoritative.events[0].event_id, completion);
    assert_eq!(authoritative.completions.len(), 1);
    assert_eq!(authoritative.completions[0].event_id, completion);
    assert_completed_plan(&mut source);

    merge_todos(&source, &mut target);
    assert_completed_parent_at_eleven(&target);
    let before_sidecar = document(&mut target);
    assert!(
        before_sidecar.events.is_empty(),
        "Todo merge invented lifecycle authority"
    );
    assert!(
        before_sidecar.completions.is_empty(),
        "Todo merge invented a completion receipt"
    );
    assert!(plans::list(&mut target, DAY).unwrap().current.is_empty());

    merge_planning(&mut target, &authoritative);
    assert_eq!(document(&mut target), authoritative);
    assert_completed_plan(&mut target);
    assert!(!document(&mut target)
        .events
        .iter()
        .any(|event| event.event_id == event_id(11, &writer, true)));
    merge_todos(&source, &mut target);
    merge_planning(&mut target, &authoritative);
    assert_eq!(document(&mut target), authoritative);
    assert_local_reopen_still_emits_event(&mut target, &completion);
}

#[test]
fn remote_unplanned_completion_never_credits_a_receivers_stale_plan() {
    let mut source = database();
    let mut target = database();
    add_plan(&mut target);
    let stale_plan = document(&mut target);
    complete_then_edit_title(&source);
    let authoritative = document(&mut source);
    assert_eq!(authoritative.events.len(), 1);
    assert!(authoritative.plans.is_empty());
    assert!(authoritative.completions.is_empty());

    merge_todos(&source, &mut target);
    assert_completed_parent_at_eleven(&target);
    assert!(document(&mut target).events.is_empty());
    assert!(document(&mut target).completions.is_empty());
    merge_planning(&mut target, &authoritative);
    let merged = document(&mut target);
    assert_eq!(merged.events, authoritative.events);
    assert_eq!(merged.plans, stale_plan.plans);
    assert!(merged.completions.is_empty());
    assert!(plans::list(&mut target, DAY).unwrap().current.is_empty());

    source
        .execute(
            "UPDATE todos SET completed=0,completed_at=NULL,updated_at=12 WHERE uuid=?1",
            [TASK],
        )
        .unwrap();
    merge_todos(&source, &mut target);
    let reopened = document(&mut source);
    assert_eq!(reopened.events.len(), 2);
    merge_planning(&mut target, &reopened);
    merge_planning(&mut target, &stale_plan);
    let current = document(&mut target);
    assert_eq!(current.events, reopened.events);
    assert_eq!(current.plans, stale_plan.plans);
    assert!(current.completions.is_empty());
    assert!(plans::list(&mut target, DAY).unwrap().current.is_empty());
}

#[test]
fn v7_restore_of_modified_completed_parent_keeps_authoritative_events_and_receipt() {
    let mut source = database();
    let mut target = database();
    add_plan(&mut source);
    merge_planning(&mut target, &document(&mut source));
    complete_then_edit_title(&source);
    let authoritative = document(&mut source);
    assert_eq!(authoritative.events.len(), 1);
    assert_eq!(authoritative.completions.len(), 1);

    let raw = data_exchange::planning_test_export(&mut source).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(wire["format_version"], 7);
    assert_eq!(wire["todos"][0]["updated_at"], 11);
    assert_eq!(
        wire["daily_planning"],
        serde_json::to_value(&authoritative).unwrap()
    );
    data_exchange::planning_test_import(&mut target, &raw).unwrap();
    assert_completed_parent_at_eleven(&target);
    assert_eq!(document(&mut target), authoritative);
    assert_completed_plan(&mut target);
    data_exchange::planning_test_import(&mut target, &raw).unwrap();
    assert_eq!(document(&mut target), authoritative);
    assert_local_reopen_still_emits_event(&mut target, &authoritative.events[0].event_id);
}

#[test]
fn failed_remote_apply_restores_marker_before_caller_rollback_and_local_completion() {
    const KEY: &str = "daily.plan.remote-apply.v1";
    for previous in [None, Some("0")] {
        let mut db = database();
        add_plan(&mut db);
        let writer = crate::db::device_id(&db).unwrap();
        if let Some(value) = previous {
            db.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![KEY, value],
            )
            .unwrap();
        }
        let tx = db.transaction().unwrap();
        let result: Result<(), String> = plans::without_lifecycle_events(&tx, || {
            let marker: String = tx
                .query_row(
                    "SELECT value FROM app_metadata WHERE key=?1",
                    [KEY],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(marker, "1");
            tx.execute(
                "UPDATE todos SET title='Uncommitted remote edit',updated_at=9 WHERE uuid=?1",
                [TASK],
            )
            .unwrap();
            Err("INJECTED_REMOTE_APPLY_FAILURE".into())
        });
        assert_eq!(result.unwrap_err(), "INJECTED_REMOTE_APPLY_FAILURE");
        let marker: Option<String> = tx
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [KEY],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(marker.as_deref(), previous);

        // Prove cleanup happened in the helper, not as a side effect of caller rollback.
        tx.execute(
            "UPDATE todos SET completed=1,completed_at=10,updated_at=10 WHERE uuid=?1",
            [TASK],
        )
        .unwrap();
        let during = plans::read_in_transaction(&tx).unwrap().document;
        assert_eq!(during.events.len(), 1);
        assert_eq!(during.events[0].event_id, event_id(10, &writer, true));
        assert_eq!(during.completions.len(), 1);
        assert_eq!(during.completions[0].event_id, during.events[0].event_id);
        tx.rollback().unwrap();

        let after = document(&mut db);
        assert!(after.events.is_empty());
        assert!(after.completions.is_empty());
        let marker: Option<String> = db
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [KEY],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(marker.as_deref(), previous);
    }
}
