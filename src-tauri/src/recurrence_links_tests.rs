use super::*;
use crate::recurrence_protocol::{parse_document, RecurrenceRule};

fn fixtures() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!("../../docs/fixtures/recurrence-links-v1.json")).unwrap()
}
fn fixture(id: &str) -> serde_json::Value {
    fixtures().into_iter().find(|f| f["id"] == id).unwrap()
}
fn setup(id: &str) -> (Connection, RecurrenceDocument) {
    let fixture = fixture(id);
    let document = parse_document(&fixture["document"].to_string()).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    recurrence_store::merge(&mut db, &document).unwrap();
    let todos: Vec<LinkTodo> = serde_json::from_value(fixture["todos"].clone()).unwrap();
    for todo in todos {
        insert(&db, &todo);
    }
    (db, document)
}
fn insert(db: &Connection, todo: &LinkTodo) {
    db.execute("INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,updated_by,completed,deleted_at,archived_at,repeat_rule,repeat_series_uuid,due_date)
      VALUES(?1,'keep title','keep note',77,1,?2,'old-device',?3,?4,?5,?6,?7,'2026-09-08')",
      params![todo.uuid,todo.updated_at,todo.completed,todo.deleted_at,todo.archived_at,todo.repeat_rule,todo.repeat_series_uuid]).unwrap();
}
fn state(db: &Connection) -> String {
    [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .map(|table| {
        let mut stmt = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let count = stmt.column_count();
        let values = stmt
            .query_map([], |r| {
                Ok((0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i).unwrap())
                    .collect::<Vec<_>>())
            })
            .unwrap();
        format!(
            "{:?}",
            values.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        )
    })
    .collect()
}

#[test]
fn shared_link_plans_and_atomic_preparation() {
    for case in fixtures() {
        let document = parse_document(&case["document"].to_string()).unwrap();
        let todos: Vec<LinkTodo> = serde_json::from_value(case["todos"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(inspect_links(&document, &todos).unwrap()).unwrap(),
            case["expected"],
            "{}",
            case["id"]
        );
        let (mut db, _) = setup(case["id"].as_str().unwrap());
        let before = state(&db);
        let rule_revision = recurrence_store::snapshot(&db).unwrap().revision;
        let prepared = prepare_links(&mut db, 5, "local-device").unwrap();
        assert_eq!(prepared.rule_revision, rule_revision);
        assert_eq!(
            prepared.plan.can_prepare,
            case["expected"]["can_prepare"].as_bool().unwrap()
        );
        if !prepared.plan.can_prepare
            || !case["expected"]["links"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["state"] == "bind")
        {
            assert_eq!(state(&db), before, "{}", case["id"]);
        }
        for uuid in &prepared.bound_todos {
            let (title, note, date, updated, device): (String, String, String, i64, String) = db
                .query_row(
                    "SELECT title,note,due_date,updated_at,updated_by FROM todos WHERE uuid=?1",
                    [uuid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .unwrap();
            assert_eq!(
                (
                    title.as_str(),
                    note.as_str(),
                    date.as_str(),
                    updated,
                    device.as_str()
                ),
                (
                    "keep title",
                    "keep note",
                    "2026-09-08",
                    1001,
                    "local-device"
                )
            );
        }
        if !prepared.bound_todos.is_empty() {
            let dirty: String = db
                .query_row(
                    "SELECT dirty_domains FROM sync_runtime_state WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(dirty.contains("todos"));
            assert_eq!(
                recurrence_store::snapshot(&db).unwrap().revision,
                rule_revision
            );
        }
        let after = state(&db);
        assert!(prepare_links(&mut db, 9999, "local-device")
            .unwrap()
            .bound_todos
            .is_empty());
        assert_eq!(state(&db), after, "Replays must not dirty any domain");
    }
}

#[test]
fn link_failure_rolls_back_earlier_binding_and_dirty() {
    let (mut db, document) = setup("all-bind");
    let last = &document
        .rules
        .iter()
        .max_by_key(|r| &r.uuid)
        .unwrap()
        .current_todo_uuid;
    db.execute_batch(&format!("CREATE TRIGGER fail_link BEFORE UPDATE ON todos WHEN NEW.uuid='{last}' BEGIN SELECT RAISE(ABORT,'injected failure'); END;")).unwrap();
    let before = state(&db);
    assert!(prepare_links(&mut db, 2000, "device").is_err());
    assert_eq!(state(&db), before);
    db.execute_batch("DROP TRIGGER fail_link").unwrap();
    assert_eq!(
        prepare_links(&mut db, 2000, "device")
            .unwrap()
            .bound_todos
            .len(),
        2
    );
}

#[test]
fn delayed_task_can_be_prepared_without_regenerating_or_changing_schedule() {
    let (mut db, _) = setup("rule-before-task");
    assert!(
        !prepare_links(&mut db, 2000, "device")
            .unwrap()
            .plan
            .can_prepare
    );
    let first = fixture("first-needs-binding");
    insert(
        &db,
        &serde_json::from_value(first["todos"][0].clone()).unwrap(),
    );
    assert_eq!(
        prepare_links(&mut db, 2000, "device")
            .unwrap()
            .bound_todos
            .len(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn rejects_clock_overflow_and_active_series_ownership_collision() {
    let (mut db, mut document) = setup("generated-ready-without-first-task");
    let mut first: RecurrenceRule =
        serde_json::from_value(fixture("ready")["document"]["rules"][0].clone()).unwrap();
    first.uuid = "123e4567-e89b-42d3-a456-426614174009".into();
    document.rules.push(first);
    let plan = inspect_links(&document, &[]).unwrap();
    assert!(plan.links.iter().all(|l| l.state == "conflict"));
    let before = state(&db);
    for device in ["", "line\n", "secret value"] {
        assert!(prepare_links(&mut db, 10, device).is_err());
    }
    assert!(prepare_links(&mut db, MAX_SAFE, "device").is_err());
    assert_eq!(state(&db), before);
    let (mut db, _) = setup("first-needs-binding");
    db.execute("UPDATE todos SET updated_at=?1", [MAX_SAFE])
        .unwrap();
    let before = state(&db);
    assert!(prepare_links(&mut db, 10, "device").is_err());
    assert_eq!(state(&db), before);
}
