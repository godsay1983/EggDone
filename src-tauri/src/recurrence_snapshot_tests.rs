use super::*;
use crate::recurrence_protocol::{parse_document, RecurrenceDocument};
use rusqlite::params;

const DEVICE: &str = "00000000-0000-4000-8000-00000000000b";
const FIRST: &str = "123e4567-e89b-42d3-a456-426614174001";
const NEXT: &str = "e9cb6d09-664e-5c54-bdf7-69f395808585";

pub(crate) fn setup(completed: bool, timed: bool, terminal: bool) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db.execute(
        "UPDATE app_metadata SET value=?1 WHERE key='device_id'",
        [DEVICE],
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule = fixture["base_rule"].clone();
    if timed {
        rule["timezone_id"] = "Asia/Shanghai".into();
        rule["schedule"]["local_time_minutes"] = 570.into();
    }
    if terminal {
        rule["schedule"]["end_type"] = "count".into();
        rule["schedule"]["max_occurrences"] = 1.into();
    }
    let document =
        parse_document(&serde_json::json!({"format_version":1,"rules":[rule]}).to_string())
            .unwrap();
    recurrence_store::merge(&mut db, &document).unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,completed,due_date,due_at,reminder_at)
        VALUES(?1,'keep title',0,1,1,?2,?3,?4,?5,?6)", params![FIRST,DEVICE,completed,
        if timed { None } else { Some("2026-09-05") },
        if timed { Some(1788571800000_i64) } else { None },
        if timed { Some(1788571500000_i64) } else { None }]).unwrap();
    db
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
        let rows = stmt
            .query_map([], |r| {
                Ok((0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i).unwrap())
                    .collect::<Vec<_>>())
            })
            .unwrap();
        format!("{:?}", rows.collect::<rusqlite::Result<Vec<_>>>().unwrap())
    })
    .collect()
}

fn check(
    snapshot: &RecurrenceUploadSnapshot,
    db: &Connection,
) -> (serde_json::Value, RecurrenceDocument) {
    let todo: sync::SyncDocument = serde_json::from_str(&snapshot.todo_json).unwrap();
    sync::validate_document(&todo).unwrap();
    let rules = parse_document(&snapshot.rules_json).unwrap();
    let json: serde_json::Value = serde_json::from_str(&snapshot.todo_json).unwrap();
    for rule in rules
        .rules
        .iter()
        .filter(|r| !r.exhausted && r.deleted_at.is_none())
    {
        let current = json["todos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["uuid"] == rule.current_todo_uuid)
            .unwrap();
        assert_eq!(current["completed"], false);
        assert!(current["deleted_at"].is_null());
        assert_eq!(current["repeat_series_uuid"], rule.first_todo_uuid);
    }
    let revision: i64 = db
        .query_row(
            "SELECT todos_dirty_version FROM sync_runtime_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(snapshot.todo_revision, revision);
    assert_eq!(
        snapshot.rule_revision,
        recurrence_store::snapshot(db).unwrap().revision
    );
    (json, rules)
}

#[test]
fn binding_snapshot_and_detached_payload() {
    let mut db = setup(false, false, false);
    let snapshot = prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap();
    check(&snapshot, &db);
    assert_eq!(snapshot.advanced_count, 0);
    let saved = state(&db);
    let replay = prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap();
    assert_eq!(state(&db), saved);
    assert_eq!(replay.todo_json, snapshot.todo_json);
    assert_eq!(replay.rules_json, snapshot.rules_json);
    db.execute("UPDATE todos SET title='later edit',updated_at=3000", [])
        .unwrap();
    assert!(snapshot.todo_json.contains("keep title"));
    let later = prepare_snapshot(&mut db, 4000).unwrap().snapshot.unwrap();
    assert!(later.todo_revision > snapshot.todo_revision);
    assert!(later.todo_json.contains("later edit"));
}

#[test]
fn completed_and_terminal_snapshots_are_consistent() {
    for (timed, terminal) in [(false, false), (true, false), (false, true)] {
        let mut db = setup(true, timed, terminal);
        let snapshot = prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap();
        let (todo, rules) = check(&snapshot, &db);
        assert_eq!(snapshot.advanced_count, 1);
        assert_eq!(rules.rules[0].exhausted, terminal);
        assert_eq!(
            todo["todos"].as_array().unwrap().len(),
            if terminal { 1 } else { 2 }
        );
        if !terminal {
            let next = todo["todos"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["uuid"] == rules.rules[0].current_todo_uuid)
                .unwrap();
            assert_eq!(next["title"], "keep title");
            if timed {
                assert_eq!(
                    next["reminder_at"].as_i64().unwrap(),
                    next["due_at"].as_i64().unwrap() - 300000
                );
            } else {
                assert_eq!(next["due_date"], "2026-09-06");
                assert!(next["due_at"].is_null());
            }
        }
        let before = state(&db);
        assert_eq!(
            prepare_snapshot(&mut db, 9000)
                .unwrap()
                .snapshot
                .unwrap()
                .advanced_count,
            0
        );
        assert_eq!(state(&db), before);
    }
}

#[test]
fn missing_conflict_and_archived_sources_do_not_mutate() {
    for sql in [
        "DELETE FROM todos",
        "UPDATE todos SET archived_at=10,updated_at=10",
        "UPDATE todos SET repeat_series_uuid='123e4567-e89b-42d3-a456-426614174009'",
    ] {
        let mut db = setup(false, false, false);
        db.execute_batch(sql).unwrap();
        let before = state(&db);
        assert!(prepare_snapshot(&mut db, 2000).unwrap().snapshot.is_none());
        assert_eq!(state(&db), before);
    }
}

fn insert_next(db: &Connection, completed: bool, archived: Option<i64>) {
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,completed,archived_at,repeat_series_uuid,due_date)
        VALUES(?1,'manual next',1,1,1000,?2,?3,?4,?5,'2026-12-25')",params![NEXT,DEVICE,completed,archived,FIRST]).unwrap();
}

#[test]
fn existing_next_is_preserved_or_reconciled_and_late_block_rolls_back() {
    for (completed, archived) in [(false, None), (true, None), (false, Some(1000))] {
        let mut db = setup(true, false, false);
        insert_next(&db, completed, archived);
        let before = state(&db);
        let result = prepare_snapshot(&mut db, 2000).unwrap();
        if archived.is_some() {
            assert!(result.snapshot.is_none());
            assert_eq!(state(&db), before);
        } else {
            let snapshot = result.snapshot.unwrap();
            let (todo, _) = check(&snapshot, &db);
            assert_eq!(snapshot.advanced_count, if completed { 2 } else { 1 });
            let next = todo["todos"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["uuid"] == NEXT)
                .unwrap();
            assert_eq!(next["title"], "manual next");
            assert_eq!(next["due_date"], "2026-12-25");
        }
    }
}

#[test]
fn invalid_document_or_write_failure_rolls_back_bindings_and_advances() {
    for sql in [
        "UPDATE todos SET created_at=-1",
        "CREATE TRIGGER injected BEFORE UPDATE ON recurrence_rules BEGIN SELECT RAISE(ABORT,'injected'); END",
        "CREATE TRIGGER injected BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'recurrence.instance.v1:%' BEGIN SELECT RAISE(ABORT,'injected'); END",
    ] {
        let mut db = setup(true,false,false);
        db.execute_batch(sql).unwrap();
        let before = state(&db);
        assert!(prepare_snapshot(&mut db,2000).is_err());
        assert_eq!(state(&db),before);
    }
}

#[test]
fn reconcile_budget_is_atomic_at_the_boundary() {
    for finish_at in [129, 130] {
        let mut db = setup(true, false, false);
        // Synthetic imported completed chain, produced inside the same transaction.
        db.execute_batch(&format!("CREATE TRIGGER completed_chain AFTER INSERT ON todos WHEN (SELECT COUNT(*) FROM todos)<{finish_at} BEGIN UPDATE todos SET completed=1 WHERE uuid=NEW.uuid; END")).unwrap();
        let before = state(&db);
        let result = prepare_snapshot(&mut db, 2000);
        if finish_at == 129 {
            let snapshot = result.unwrap().snapshot.unwrap();
            assert_eq!(snapshot.advanced_count, MAX_RECONCILE_STEPS);
            check(&snapshot, &db);
        } else {
            assert_eq!(result.unwrap_err(), "RECURRENCE_RECONCILE_LIMIT");
            assert_eq!(state(&db), before);
        }
    }
}

#[test]
fn purged_next_receipt_blocks_snapshot_without_resurrection() {
    let mut db = setup(true, false, false);
    let original_rule = recurrence_store::snapshot(&db).unwrap().document;
    prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap();
    db.execute("DELETE FROM todos WHERE uuid=?1", [NEXT])
        .unwrap();
    let mut stale = original_rule;
    stale.rules[0].updated_at = 9000;
    recurrence_store::merge(&mut db, &stale).unwrap();
    let before = state(&db);
    let result = prepare_snapshot(&mut db, 10000).unwrap();
    assert!(result.snapshot.is_none());
    assert_eq!(result.links.links[0].state, "missing");
    assert_eq!(state(&db), before);
}

#[test]
fn failed_commit_rolls_back_payload_changes() {
    let mut db = setup(true, false, false);
    db.execute_batch("PRAGMA foreign_keys=ON;
        CREATE TABLE commit_parent(id INTEGER PRIMARY KEY);
        CREATE TABLE commit_child(id INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED);
        CREATE TRIGGER injected_commit AFTER INSERT ON todos BEGIN INSERT INTO commit_child VALUES(1); END;").unwrap();
    let before = state(&db);
    assert_eq!(
        prepare_snapshot(&mut db, 2000).unwrap_err(),
        "RECURRENCE_DATABASE"
    );
    assert_eq!(state(&db), before);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM commit_child", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER injected_commit").unwrap();
    check(
        &prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap(),
        &db,
    );
}

#[test]
fn guards_and_empty_rule_snapshot() {
    let mut db = setup(false, false, false);
    let before = state(&db);
    for now in [-1, MAX_SAFE] {
        assert!(prepare_snapshot(&mut db, now).is_err());
    }
    assert_eq!(state(&db), before);
    db.execute("DELETE FROM app_metadata WHERE key='device_id'", [])
        .unwrap();
    assert_eq!(
        prepare_snapshot(&mut db, 2000).unwrap_err(),
        "RECURRENCE_DEVICE_MISSING"
    );
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('device_id',?1)",
        [DEVICE],
    )
    .unwrap();
    db.execute("DELETE FROM recurrence_rules", []).unwrap();
    let before = state(&db);
    let snapshot = prepare_snapshot(&mut db, 2000).unwrap().snapshot.unwrap();
    check(&snapshot, &db);
    assert_eq!(snapshot.advanced_count, 0);
    assert_eq!(state(&db), before);
}
