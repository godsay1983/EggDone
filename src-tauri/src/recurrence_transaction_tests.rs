use super::*;
use crate::recurrence_protocol::RecurrenceDocument;

fn setup(timed: bool, last: bool) -> (Connection, RecurrenceRule) {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule: RecurrenceRule = serde_json::from_value(fixture["base_rule"].clone()).unwrap();
    if timed {
        rule.timezone_id = Some("Asia/Shanghai".into());
        rule.schedule.local_time_minutes = Some(570);
    }
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
    db.execute("INSERT INTO todos(uuid,title,note,pinned,priority,sort_order,created_at,updated_at,due_date,due_at,reminder_at,updated_by)
        VALUES(?1,'keep title','keep note',1,1,0,1,1,?2,?3,?4,'device-a')",
        params![rule.first_todo_uuid, if timed { None } else { Some("2026-09-05") },
            if timed { Some(1788571800000_i64) } else { None }, if timed { Some(1788571500000_i64) } else { None }]).unwrap();
    (db, rule)
}
fn request(rule: &RecurrenceRule) -> AdvanceRequest<'_> {
    AdvanceRequest {
        rule_uuid: &rule.uuid,
        current_todo_uuid: &rule.first_todo_uuid,
        action: RecurrenceAction::Complete,
        now: 2000,
        device_id: "device-b",
        reminder_at: None,
    }
}
fn dump(db: &Connection) -> Vec<String> {
    let mut output = Vec::new();
    for table in [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ] {
        let mut statement = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                Ok((0..count)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap();
        output.extend(rows.map(Result::unwrap));
    }
    output
}
fn insert_existing(
    db: &Connection,
    rule: &RecurrenceRule,
    uuid: &str,
    belongs: bool,
    deleted: bool,
) {
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,repeat_series_uuid,deleted_at,updated_by)
        VALUES(?1,'remote edited title',-50,5,9000,?2,?3,'device-z')",
        params![uuid, if belongs { Some(&rule.first_todo_uuid) } else { None }, if deleted { Some(9000) } else { None }]).unwrap();
}
#[test]
fn recurrence_transaction_completes_and_replays_without_duplicate() {
    let (mut db, rule) = setup(true, false);
    let mut req = request(&rule);
    req.reminder_at = Some(1788571500000);
    let result = advance_current(&mut db, &req).unwrap().unwrap();
    let next = result.next.unwrap();
    let row = db.query_row("SELECT title,note,pinned,priority,completed,due_at,reminder_at,repeat_rule,repeat_series_uuid FROM todos WHERE uuid=?1", [&next.uuid],
        |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,i64>(3)?,r.get::<_,bool>(4)?,r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,String>(8)?))).unwrap();
    assert_eq!(
        row,
        (
            "keep title".into(),
            "keep note".into(),
            true,
            1,
            false,
            next.due_at.unwrap(),
            next.due_at.unwrap() - 300000,
            None,
            rule.first_todo_uuid.clone()
        )
    );
    let before = dump(&db);
    assert!(advance_current(&mut db, &req).unwrap().is_none());
    assert_eq!(dump(&db), before);
    assert!(recurrence_store::snapshot(&db).unwrap().revision > 1);
    assert!(db
        .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .contains("todos"));
}
#[test]
fn recurrence_transaction_all_day_and_terminal() {
    for last in [false, true] {
        let (mut db, rule) = setup(false, last);
        let plan = advance_current(&mut db, &request(&rule)).unwrap().unwrap();
        assert_eq!(plan.rule.exhausted, last);
        if let Some(next) = plan.next {
            assert_eq!(
                db.query_row(
                    "SELECT due_date,due_at,reminder_at FROM todos WHERE uuid=?1",
                    [next.uuid],
                    |r| Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<i64>>(2)?
                    ))
                )
                .unwrap(),
                ("2026-09-06".into(), None, None)
            );
        }
    }
}
#[test]
fn recurrence_transaction_skip_and_reconcile_evidence() {
    for (action, completed, archived, deleted, advances) in [
        (RecurrenceAction::Skip, false, false, false, true),
        (RecurrenceAction::Reconcile, true, true, false, true),
        (RecurrenceAction::Reconcile, false, true, false, false),
        (RecurrenceAction::Reconcile, false, false, true, true),
        (RecurrenceAction::Complete, false, true, false, false),
    ] {
        let (mut db, rule) = setup(false, false);
        db.execute(
            "UPDATE todos SET completed=?1,archived_at=?2,deleted_at=?3",
            params![completed, archived.then_some(1000), deleted.then_some(1000)],
        )
        .unwrap();
        let before = dump(&db);
        let mut req = request(&rule);
        req.action = action;
        let result = advance_current(&mut db, &req).unwrap();
        assert_eq!(result.is_some(), advances);
        if !advances {
            assert_eq!(dump(&db), before);
        }
    }
}
#[test]
fn recurrence_transaction_failure_rolls_back_every_domain_and_receipt() {
    for sql in [
        "CREATE TRIGGER injected BEFORE INSERT ON todos BEGIN SELECT RAISE(ABORT,'injected'); END",
        "CREATE TRIGGER injected BEFORE UPDATE ON recurrence_rules BEGIN SELECT RAISE(ABORT,'injected'); END",
        "CREATE TRIGGER injected BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'recurrence.instance.v1:%' BEGIN SELECT RAISE(ABORT,'injected'); END",
        "CREATE TRIGGER injected BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'reminder.complete.v1:%' BEGIN SELECT RAISE(ABORT,'injected'); END",
    ] {
        let (mut db, rule)=setup(true,false);let before=dump(&db);
        db.execute_batch(sql).unwrap();let mut req=request(&rule);req.reminder_at=Some(1788571500000);
        assert!(advance_current(&mut db,&req).is_err());assert_eq!(dump(&db),before);
        db.execute_batch("DROP TRIGGER injected").unwrap();
        assert!(advance_current(&mut db,&req).unwrap().is_some());
    }
}
#[test]
fn recurrence_transaction_existing_next_is_preserved_or_conflict_rolls_back() {
    for (belongs, deleted) in [(true, false), (true, true), (false, false)] {
        let (mut db, rule) = setup(false, false);
        let evidence = CompletionEvidence {
            uuid: rule.first_todo_uuid.clone(),
            completed: true,
            deleted_at: None,
            archived_at: None,
        };
        let next = plan_recurrence_advance(&rule, Some(&evidence), 2000, "device-b")
            .unwrap()
            .unwrap()
            .next
            .unwrap();
        insert_existing(&db, &rule, &next.uuid, belongs, deleted);
        let before = dump(&db);
        let result = advance_current(&mut db, &request(&rule));
        if belongs {
            assert!(result.unwrap().is_some());
            assert_eq!(
                db.query_row(
                    "SELECT title,updated_at,deleted_at FROM todos WHERE uuid=?1",
                    [&next.uuid],
                    |r| Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<i64>>(2)?
                    ))
                )
                .unwrap(),
                ("remote edited title".into(), 9000, deleted.then_some(9000))
            );
        } else {
            assert!(result.is_err());
            assert_eq!(dump(&db), before);
        }
    }
}
#[test]
fn recurrence_transaction_stale_missing_and_legacy_do_not_change_data() {
    for case in [
        "missing",
        "stale-reminder",
        "legacy",
        "bad-device",
        "invalid-clock",
        "stopped",
        "wrong-series",
    ] {
        let (mut db, rule) = setup(true, false);
        match case {
            "missing" => {
                db.execute("DELETE FROM todos", []).unwrap();
            }
            "legacy" => {
                db.execute("UPDATE todos SET repeat_rule='daily'", [])
                    .unwrap();
            }
            "wrong-series" => {
                db.execute("UPDATE todos SET repeat_series_uuid='another'", [])
                    .unwrap();
            }
            "stopped" => {
                let mut stopped = rule.clone();
                stopped.deleted_at = Some(1000);
                recurrence_store::merge(
                    &mut db,
                    &RecurrenceDocument {
                        format_version: 1,
                        rules: vec![stopped],
                    },
                )
                .unwrap();
            }
            _ => (),
        }
        let before = dump(&db);
        let mut req = request(&rule);
        if case == "stale-reminder" {
            req.reminder_at = Some(123);
        }
        if case == "bad-device" {
            req.device_id = "bad device";
        }
        if case == "invalid-clock" {
            req.now = -1;
        }
        let result = advance_current(&mut db, &req);
        if ["legacy", "bad-device", "invalid-clock", "wrong-series"].contains(&case) {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_none());
        }
        assert_eq!(dump(&db), before, "{case}");
    }
}
#[test]
fn recurrence_transaction_purged_receipt_does_not_resurrect() {
    let (mut db, rule) = setup(false, false);
    let plan = advance_current(&mut db, &request(&rule)).unwrap().unwrap();
    db.execute("DELETE FROM todos WHERE uuid=?1", [plan.next.unwrap().uuid])
        .unwrap();
    db.execute(
        "UPDATE recurrence_rules SET current_todo_uuid=?1,record_json=?2",
        params![rule.first_todo_uuid, serde_json::to_string(&rule).unwrap()],
    )
    .unwrap();
    let before = dump(&db);
    assert!(advance_current(&mut db, &request(&rule)).is_err());
    assert_eq!(dump(&db), before);
}
#[test]
fn recurrence_transaction_clock_rollback_and_late_reminder() {
    let (mut db, rule) = setup(true, false);
    let mut req = request(&rule);
    req.now = 500;
    let plan = advance_current(&mut db, &req).unwrap().unwrap();
    assert_eq!(plan.rule.updated_at, 1001);
    let (mut db, rule) = setup(true, false);
    let mut req = request(&rule);
    req.now = 1893456000000;
    let plan = advance_current(&mut db, &req).unwrap().unwrap();
    assert!(db
        .query_row(
            "SELECT reminder_at FROM todos WHERE uuid=?1",
            [plan.next.unwrap().uuid],
            |r| r.get::<_, Option<i64>>(0)
        )
        .unwrap()
        .is_none());
}
