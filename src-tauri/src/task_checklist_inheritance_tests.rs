use crate::{
    recurrence::{recurrence_on_date, recurrence_todo_uuid},
    recurrence_protocol::{RecurrenceDocument, RecurrenceRule},
    recurrence_store,
    recurrence_transaction::{advance_current, AdvanceRequest, RecurrenceAction},
    task_checklist_inheritance::*,
    task_checklist_protocol::*,
    task_checklist_store as store,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
fn fixtures() -> (Value, Value) {
    (
        serde_json::from_str(include_str!(
            "../../docs/fixtures/task-checklist-editor-v1.json"
        ))
        .unwrap(),
        serde_json::from_str(include_str!(
            "../../docs/fixtures/task-checklist-inheritance-v1.json"
        ))
        .unwrap(),
    )
}
fn setup(defined: bool) -> (Connection, RecurrenceRule, ChecklistDefinition) {
    let (f, i) = fixtures();
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let rule = RecurrenceRule {
        uuid: f["rule_uuid"].as_str().unwrap().into(),
        first_todo_uuid: f["todo_uuid"].as_str().unwrap().into(),
        current_todo_uuid: f["todo_uuid"].as_str().unwrap().into(),
        schedule: serde_json::from_value(f["schedule"].clone()).unwrap(),
        timezone_id: None,
        current_date: "2026-09-20".into(),
        generated_count: 1,
        exhausted: false,
        updated_at: 100,
        updated_by: "device-a".into(),
        deleted_at: None,
    };
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![rule.clone()],
        },
    )
    .unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,due_date,repeat_series_uuid) VALUES(?1,'Trip',0,1,100,'device-a','2026-09-20',?1)",[&rule.first_todo_uuid]).unwrap();
    let definition = ChecklistDefinition {
        rule_uuid: rule.uuid.clone(),
        first_todo_uuid: rule.first_todo_uuid.clone(),
        schedule: rule.schedule.clone(),
        timezone_id: None,
        applies_from_index: 2,
        entries: serde_json::from_value(i["entries"].clone()).unwrap(),
        created_at: 100,
        updated_at: 100,
        updated_by: "device-a".into(),
        deleted_at: None,
    };
    if defined {
        define(&mut db, &definition);
    }
    (db, rule, definition)
}
fn define(db: &mut Connection, d: &ChecklistDefinition) {
    store::merge(
        db,
        &ItemsDocument::default(),
        &DefinitionsDocument {
            format_version: 1,
            definitions: vec![d.clone()],
        },
    )
    .unwrap();
}
fn advance(db: &mut Connection, r: &RecurrenceRule) -> String {
    advance_current(
        db,
        &AdvanceRequest {
            rule_uuid: &r.uuid,
            current_todo_uuid: &r.current_todo_uuid,
            action: RecurrenceAction::Complete,
            now: 200,
            device_id: "device-a",
            reminder_at: None,
        },
    )
    .unwrap()
    .unwrap()
    .next
    .unwrap()
    .uuid
}
fn parent(db: &Connection, r: &RecurrenceRule, date: &str) -> String {
    let o = recurrence_on_date(&r.schedule, date).unwrap();
    let id = recurrence_todo_uuid(&r.uuid, &r.schedule, &o).unwrap();
    let due = crate::recurrence_time::recurrence_due_at(
        date,
        r.schedule.local_time_minutes,
        r.timezone_id.as_deref(),
    )
    .unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,due_date,due_at,repeat_series_uuid) VALUES(?1,'Remote',0,1,100,'remote',?2,?3,?4)",
        params![id,if due.is_none() {Some(date)} else {None},due,r.first_todo_uuid]).unwrap();
    id
}
fn dump(db: &Connection) -> Vec<String> {
    [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "sync_runtime_state",
        "task_checklist_items",
        "task_checklist_definitions",
        "task_checklist_sync_state",
        "app_metadata",
    ]
    .iter()
    .flat_map(|table| {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let cols = q.column_count();
        q.query_map([], |r| {
            Ok((0..cols)
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
#[test]
fn inheritance_completion_uses_definition_and_deterministic_identity() {
    let (mut db, r, d) = setup(true);
    let id = advance(&mut db, &r);
    let f = fixtures().1;
    assert_eq!(id, f["occurrences"][0]["todo"]);
    let s = store::snapshot(&mut db).unwrap();
    assert_eq!(s.items.items.len(), 2);
    for (entry, expected) in d
        .entries
        .iter()
        .zip(f["occurrences"][0]["items"].as_array().unwrap())
    {
        let item = s
            .items
            .items
            .iter()
            .find(|i| i.source_entry_uuid.as_ref() == Some(&entry.uuid))
            .unwrap();
        assert_eq!(item.uuid, expected.as_str().unwrap());
        assert_eq!(item.content, entry.content);
        assert!(!item.completed);
        assert_eq!(item.created_at, 100);
        assert_eq!(item.updated_by, "checklist-seed-v1");
    }
    let before = dump(&db);
    assert_eq!(repair(&mut db).unwrap(), 0);
    assert_eq!(dump(&db), before);
}
#[test]
fn inheritance_late_definition_preserves_edits_and_tombstones() {
    let (mut db, r, d) = setup(false);
    let id = advance(&mut db, &r);
    define(&mut db, &d);
    db.execute(
        "UPDATE todos SET due_date='2030-01-01' WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert_eq!(repair(&mut db).unwrap(), 2);
    let mut s = store::snapshot(&mut db).unwrap().items;
    s.items[0].content = "User text".into();
    s.items[0].completed = true;
    s.items[0].updated_at = 500;
    s.items[1].deleted_at = Some(500);
    s.items[1].updated_at = 500;
    store::merge(&mut db, &s, &DefinitionsDocument::default()).unwrap();
    let before = dump(&db);
    assert_eq!(repair(&mut db).unwrap(), 0);
    assert_eq!(dump(&db), before);
}
#[test]
fn inheritance_history_without_receipt_and_stopped_rule() {
    let (mut db, mut r, d) = setup(true);
    let a = parent(&db, &r, "2026-09-21");
    parent(&db, &r, "2026-09-22");
    r.deleted_at = Some(200);
    r.updated_at = 200;
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![r],
        },
    )
    .unwrap();
    assert_eq!(repair(&mut db).unwrap(), 4);
    assert_eq!(
        store::visible_items(&mut db, &a).unwrap().len(),
        d.entries.len()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn inheritance_missing_dependencies_and_hidden_parents_wait() {
    for state in fixtures().1["suppressed_states"].as_array().unwrap() {
        let (mut db, r, _) = setup(true);
        let id = parent(&db, &r, "2026-09-21");
        let sql = match state.as_str().unwrap() {
            "deleted" => "UPDATE todos SET deleted_at=100 WHERE uuid=?1",
            "archived" => "UPDATE todos SET archived_at=100 WHERE uuid=?1",
            "legacy" => "UPDATE todos SET repeat_rule='daily' WHERE uuid=?1",
            _ => "UPDATE todos SET repeat_series_uuid=NULL WHERE uuid=?1",
        };
        db.execute(sql, [&id]).unwrap();
        let before = dump(&db);
        assert_eq!(repair(&mut db).unwrap(), 0);
        assert_eq!(dump(&db), before);
        db.execute("UPDATE todos SET deleted_at=NULL,archived_at=NULL,repeat_rule=NULL,repeat_series_uuid=?1 WHERE uuid=?2",params![r.first_todo_uuid,id]).unwrap();
        assert_eq!(repair(&mut db).unwrap(), 2);
    }
    let (mut db, r, _) = setup(true);
    parent(&db, &r, "2026-09-21");
    db.execute("DELETE FROM recurrence_rules", []).unwrap();
    let before = dump(&db);
    assert_eq!(repair(&mut db).unwrap(), 0);
    assert_eq!(dump(&db), before);
}
#[test]
fn inheritance_conflicts_and_failures_roll_back_completion() {
    for trigger in fixtures().1["failure_triggers"].as_array().unwrap() {
        let (mut db, r, _) = setup(true);
        db.execute_batch(&format!(
            "CREATE TRIGGER injected {} BEGIN SELECT RAISE(ABORT,'injected'); END;",
            trigger.as_str().unwrap()
        ))
        .unwrap();
        let before = dump(&db);
        let req = AdvanceRequest {
            rule_uuid: &r.uuid,
            current_todo_uuid: &r.current_todo_uuid,
            action: RecurrenceAction::Complete,
            now: 200,
            device_id: "device-a",
            reminder_at: None,
        };
        assert!(advance_current(&mut db, &req)
            .unwrap_err()
            .contains("injected"));
        assert_eq!(dump(&db), before);
        db.execute_batch("DROP TRIGGER injected").unwrap();
        advance(&mut db, &r);
    }
    let (mut db, r, _) = setup(true);
    let id = parent(&db, &r, "2026-09-21");
    repair(&mut db).unwrap();
    let mut i = store::snapshot(&mut db).unwrap().items.items[0].clone();
    i.created_at = 99;
    db.execute(
        "UPDATE task_checklist_items SET record_json=?1 WHERE uuid=?2",
        params![serde_json::to_string(&i).unwrap(), i.uuid],
    )
    .unwrap();
    let before = dump(&db);
    assert!(repair(&mut db).unwrap_err().contains("IDENTITY_CONFLICT"));
    assert_eq!(dump(&db), before);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM todos WHERE uuid=?1", [id], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn inheritance_timed_remote_and_invalid_receipt() {
    let (mut db, mut r, mut d) = setup(false);
    r.timezone_id = Some("Asia/Shanghai".into());
    r.schedule.local_time_minutes = Some(570);
    r.updated_at = 101;
    // Fixture replacement is isolated; production immutable-rule merge correctly rejects schedule mutation.
    db.execute("DELETE FROM recurrence_rules", []).unwrap();
    recurrence_store::merge(
        &mut db,
        &RecurrenceDocument {
            format_version: 1,
            rules: vec![r.clone()],
        },
    )
    .unwrap();
    d.schedule = r.schedule.clone();
    d.timezone_id = r.timezone_id.clone();
    define(&mut db, &d);
    let id = parent(&db, &r, "2026-09-21");
    assert_eq!(repair(&mut db).unwrap(), 2);
    let receipt = json!({"rule_uuid":r.uuid,"occurrence_key":"forged","planned_date":"2026-09-21","planned_due_at":0});
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![format!("recurrence.instance.v1:{id}"), receipt.to_string()],
    )
    .unwrap();
    let before = dump(&db);
    assert!(repair(&mut db).is_err());
    assert_eq!(dump(&db), before);
}
#[test]
fn inheritance_snapshot_repairs_late_definition_without_advancing_history() {
    let (mut db, r, d) = setup(false);
    advance(&mut db, &r);
    define(&mut db, &d);
    db.execute(
        "UPDATE todos SET updated_by='123e4567-e89b-42d3-a456-000000000003'",
        [],
    )
    .unwrap();
    let result = crate::recurrence_snapshot::prepare_snapshot(&mut db, 300).unwrap();
    assert_eq!(result.snapshot.unwrap().advanced_count, 0);
    assert_eq!(store::snapshot(&mut db).unwrap().items.items.len(), 2);
}

#[test]
fn inheritance_empty_deleted_mismatched_and_unverifiable_definitions() {
    for mode in [
        "empty",
        "deleted",
        "mismatch",
        "unverifiable",
        "over-local-limit",
    ] {
        let (mut db, r, mut d) = setup(false);
        let id = parent(&db, &r, "2026-09-21");
        match mode {
            "empty" => d.entries.clear(),
            "deleted" => d.deleted_at = Some(100),
            "mismatch" => d.schedule.interval = 2,
            "unverifiable" => {
                db.execute(
                    "UPDATE todos SET due_date='2030-01-01' WHERE uuid=?1",
                    [&id],
                )
                .unwrap();
            }
            _ => {
                d.entries = (100..125)
                    .map(|n| DefinitionEntry {
                        uuid: format!("123e4567-e89b-42d3-a456-{n:012}"),
                        content: format!("Step {n}"),
                        sort_order: n,
                    })
                    .collect()
            }
        }
        define(&mut db, &d);
        let before = dump(&db);
        if mode == "mismatch" {
            assert!(repair(&mut db)
                .unwrap_err()
                .contains("DEFINITION_RULE_MISMATCH"));
        } else if mode == "over-local-limit" {
            assert_eq!(repair(&mut db).unwrap(), 25);
            continue;
        } else {
            assert_eq!(repair(&mut db).unwrap(), 0);
        }
        assert_eq!(dump(&db), before);
    }
}
