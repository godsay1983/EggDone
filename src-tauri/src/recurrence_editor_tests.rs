use super::*;
use crate::recurrence_transaction::{advance_current, AdvanceRequest, RecurrenceAction};

const REPLACEMENT: &str = "123e4567-e89b-42d3-a456-426614174099";
const DEVICE: &str = "00000000-0000-4000-8000-00000000000b";

fn fixture() -> (Connection, RuleEditRequest) {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule: RecurrenceRule = serde_json::from_value(fixture["base_rule"].clone()).unwrap();
    rule.updated_by = DEVICE.into();
    db.execute(
        "INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,reminder_at,updated_by)
        VALUES(?1,'keep title','keep note',0,1,1,1000,'device-a')",
        [&rule.first_todo_uuid],
    )
    .unwrap();
    (
        db,
        RuleEditRequest {
            rule,
            expected_todo_updated_at: 1,
            replaces: None,
        },
    )
}

fn dump(db: &Connection) -> Vec<String> {
    [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .flat_map(|table| {
        let mut stmt = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let count = stmt.column_count();
        stmt.query_map([], |r| {
            Ok((0..count)
                .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
    })
    .collect()
}

fn advance(db: &mut Connection, rule: &RecurrenceRule) -> RecurrenceRule {
    advance_current(
        db,
        &AdvanceRequest {
            rule_uuid: &rule.uuid,
            current_todo_uuid: &rule.current_todo_uuid,
            action: RecurrenceAction::Complete,
            now: 2000,
            device_id: DEVICE,
            reminder_at: None,
        },
    )
    .unwrap()
    .unwrap()
    .rule
}

fn replacement(db: &Connection, old: &RecurrenceRule) -> RuleEditRequest {
    let mut rule = old.clone();
    rule.uuid = REPLACEMENT.into();
    rule.first_todo_uuid = old.current_todo_uuid.clone();
    rule.schedule.anchor_date = old.current_date.clone();
    rule.schedule.interval = 2;
    rule.generated_count = 1;
    rule.updated_at = 3000;
    RuleEditRequest {
        expected_todo_updated_at: db
            .query_row(
                "SELECT updated_at FROM todos WHERE uuid=?1",
                [&rule.first_todo_uuid],
                |r| r.get(0),
            )
            .unwrap(),
        rule,
        replaces: Some(old.clone()),
    }
}

#[test]
fn create_binds_existing_task_and_replay_never_rewrites_progress() {
    let (mut db, request) = fixture();
    let saved = save_rule(&mut db, &request).unwrap();
    let task: (
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        String,
    ) = db
        .query_row(
            "SELECT title,note,due_date,due_at,reminder_at,repeat_series_uuid FROM todos",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        task,
        (
            "keep title".into(),
            "keep note".into(),
            Some("2026-09-05".into()),
            None,
            None,
            saved.first_todo_uuid.clone()
        )
    );
    assert!(recurrence_store::snapshot(&db).unwrap().revision > 0);
    let before = dump(&db);
    assert_eq!(save_rule(&mut db, &request).unwrap(), saved);
    assert_eq!(dump(&db), before);
    let advanced = advance(&mut db, &saved);
    let before = dump(&db);
    assert_eq!(save_rule(&mut db, &request).unwrap(), advanced);
    assert_eq!(dump(&db), before);
}

#[test]
fn replacement_retains_history_and_new_rule_advances_from_current_task() {
    let (mut db, request) = fixture();
    let saved = save_rule(&mut db, &request).unwrap();
    let advanced = advance(&mut db, &saved);
    let edit = replacement(&db, &advanced);
    let saved = save_rule(&mut db, &edit).unwrap();
    let rules = recurrence_store::snapshot(&db).unwrap().document;
    assert!(rules
        .rules
        .iter()
        .find(|r| r.uuid == advanced.uuid)
        .unwrap()
        .deleted_at
        .is_some());
    assert_eq!(
        db.query_row(
            "SELECT completed FROM todos WHERE uuid=?1",
            [&request.rule.first_todo_uuid],
            |r| r.get::<_, bool>(0)
        )
        .unwrap(),
        true
    );
    let prepared = crate::recurrence_snapshot::prepare_snapshot(&mut db, 4000).unwrap();
    assert!(prepared.snapshot.is_some());
    let next = advance(&mut db, &saved);
    assert_eq!(next.current_date, "2026-09-08");
    let before = dump(&db);
    assert_eq!(save_rule(&mut db, &edit).unwrap(), next);
    assert_eq!(dump(&db), before);
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![advanced],
        },
    )
    .unwrap();
    assert!(crate::recurrence_snapshot::prepare_snapshot(&mut db, 5000)
        .unwrap()
        .snapshot
        .is_some());
}

#[test]
fn replacement_of_first_task_releases_unique_link_and_repeated_edit_is_rejected() {
    let (mut db, request) = fixture();
    let old = save_rule(&mut db, &request).unwrap();
    let edit = replacement(&db, &old);
    save_rule(&mut db, &edit).unwrap();
    let mut other = edit.clone();
    other.rule.uuid = "123e4567-e89b-42d3-a456-426614174098".into();
    let before = dump(&db);
    assert!(save_rule(&mut db, &other).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn invalid_or_stale_task_rejects_without_writes() {
    for sql in [
        "UPDATE todos SET completed=1",
        "UPDATE todos SET deleted_at=1",
        "UPDATE todos SET archived_at=1",
        "UPDATE todos SET updated_at=2",
        "UPDATE todos SET repeat_rule='daily'",
        "UPDATE todos SET repeat_series_uuid='unknown'",
        "DELETE FROM todos",
    ] {
        let (mut db, request) = fixture();
        db.execute(sql, []).unwrap();
        let before = dump(&db);
        assert!(save_rule(&mut db, &request).is_err(), "{sql}");
        assert_eq!(dump(&db), before);
    }
}

#[test]
fn stale_rule_snapshot_rejects_even_if_task_is_unchanged() {
    let (mut db, request) = fixture();
    let old = save_rule(&mut db, &request).unwrap();
    let edit = replacement(&db, &old);
    let mut changed = old.clone();
    changed.updated_at += 1;
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![changed],
        },
    )
    .unwrap();
    let before = dump(&db);
    assert_eq!(
        save_rule(&mut db, &edit).unwrap_err(),
        "RECURRENCE_EDIT_CONFLICT"
    );
    assert_eq!(dump(&db), before);
}

#[test]
fn changed_request_or_remote_uuid_collision_cannot_reuse_local_receipt() {
    let (mut db, mut request) = fixture();
    save_rule(&mut db, &request).unwrap();
    request.rule.schedule.interval = 2;
    let before = dump(&db);
    assert_eq!(
        save_rule(&mut db, &request).unwrap_err(),
        "RECURRENCE_EDIT_CONFLICT"
    );
    assert_eq!(dump(&db), before);
    db.execute(
        "DELETE FROM app_metadata WHERE key LIKE 'recurrence.edit.v1:%'",
        [],
    )
    .unwrap();
    let before = dump(&db);
    assert!(save_rule(&mut db, &request).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn replacement_rolls_back_every_write_boundary_and_allows_retry() {
    for trigger in [
        "BEFORE UPDATE ON recurrence_rules",
        "BEFORE UPDATE ON todos",
        "BEFORE INSERT ON recurrence_rules",
        "BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'recurrence.edit.v1:%'",
    ] {
        let (mut db, request) = fixture();
        let old = save_rule(&mut db, &request).unwrap();
        let edit = replacement(&db, &old);
        let before = dump(&db);
        db.execute_batch(&format!(
            "CREATE TRIGGER injected {trigger} BEGIN SELECT RAISE(ABORT,'injected'); END;"
        ))
        .unwrap();
        assert!(save_rule(&mut db, &edit).unwrap_err().contains("injected"));
        assert_eq!(dump(&db), before);
        db.execute_batch("DROP TRIGGER injected").unwrap();
        assert!(save_rule(&mut db, &edit).is_ok());
    }
}

#[test]
fn timed_creation_checks_timezone_and_persists_real_epoch() {
    let (mut db, mut request) = fixture();
    request.rule.schedule.local_time_minutes = Some(570);
    request.rule.timezone_id = Some("Asia/Shanghai".into());
    save_rule(&mut db, &request).unwrap();
    assert_eq!(
        db.query_row("SELECT due_at FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1788571800000
    );
    let (mut db, mut request) = fixture();
    request.rule.schedule.local_time_minutes = Some(570);
    request.rule.timezone_id = Some("Missing/Zone".into());
    let before = dump(&db);
    assert_eq!(
        save_rule(&mut db, &request).unwrap_err(),
        "RECURRENCE_TIMEZONE_UNSUPPORTED"
    );
    assert_eq!(dump(&db), before);
}

#[test]
fn stop_is_monotonic_idempotent_preserves_tasks_and_rejects_stale_editor() {
    let (mut db, request) = fixture();
    let old = save_rule(&mut db, &request).unwrap();
    let task_before: String = db
        .query_row("SELECT title || updated_at FROM todos", [], |r| r.get(0))
        .unwrap();
    let stopped = stop_rule(&mut db, &old, 1, "device-b").unwrap();
    assert_eq!(stopped.deleted_at, Some(old.updated_at + 1));
    let before = dump(&db);
    assert_eq!(stop_rule(&mut db, &old, 1, "device-b").unwrap(), stopped);
    assert_eq!(save_rule(&mut db, &request).unwrap(), stopped);
    assert_eq!(dump(&db), before);
    assert_eq!(
        db.query_row("SELECT title || updated_at FROM todos", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        task_before
    );
    let (mut db, request) = fixture();
    let old = save_rule(&mut db, &request).unwrap();
    advance(&mut db, &old);
    let before = dump(&db);
    assert_eq!(
        stop_rule(&mut db, &old, 5000, "device-b").unwrap_err(),
        "RECURRENCE_EDIT_CONFLICT"
    );
    assert_eq!(dump(&db), before);
}

#[test]
fn invalid_draft_and_overflow_never_partially_bind_task() {
    for case in [
        "interval",
        "device",
        "device-label",
        "device-compact",
        "clock",
        "uuid",
        "stopped",
        "overflow",
        "progress",
    ] {
        let (mut db, mut request) = fixture();
        match case {
            "interval" => request.rule.schedule.interval = 0,
            "device" => request.rule.updated_by = "invalid device".into(),
            "device-label" => request.rule.updated_by = "device-a".into(),
            "device-compact" => request.rule.updated_by = DEVICE.replace('-', ""),
            "clock" => request.rule.updated_at = -1,
            "uuid" => request.rule.uuid = "invalid".into(),
            "stopped" => request.rule.deleted_at = Some(1),
            "overflow" => {
                db.execute("UPDATE todos SET updated_at=?1", [MAX_SAFE])
                    .unwrap();
                request.expected_todo_updated_at = MAX_SAFE;
            }
            "progress" => request.rule.generated_count = 2,
            _ => unreachable!(),
        }
        let before = dump(&db);
        assert!(save_rule(&mut db, &request).is_err(), "{case}");
        assert_eq!(dump(&db), before);
    }
}
