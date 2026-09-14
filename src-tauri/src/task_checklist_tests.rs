use crate::task_checklist_protocol::*;
use crate::task_checklist_store as store;
use rusqlite::Connection;
use serde_json::{json, Value};
fn fixtures() -> Value {
    let raw: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(include_str!("../../docs/fixtures/task-checklist-v1.json")).unwrap();
    Value::Object(
        raw.into_iter()
            .filter(|(k, _)| k != "invalid")
            .map(|(k, v)| (k, serde_json::from_str(v.get()).unwrap()))
            .collect(),
    )
}
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
fn item_doc(v: &Value) -> ItemsDocument {
    parse_items(&v.to_string()).unwrap()
}
#[test]
fn production_protocol_fixtures() {
    let f = fixtures();
    for c in f["identities"].as_array().unwrap() {
        assert_eq!(
            item_uuid(c["todo"].as_str().unwrap(), c["entry"].as_str().unwrap()).unwrap(),
            c["expected"]
        );
    }
    for kind in ["items", "definitions"] {
        let parse = |v: &Value| -> Result<Value, String> {
            let t = v.to_string();
            let s = if kind == "items" {
                encode_items(&parse_items(&t)?)?
            } else {
                encode_definitions(&parse_definitions(&t)?)?
            };
            Ok(serde_json::from_str(&s).unwrap())
        };
        for c in f["valid"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == kind)
        {
            assert!(parse(&c["document"]).is_ok(), "{}", c["id"]);
        }
        // Preserve invalid UTF-16 escapes verbatim until they reach the production parser.
        let raw: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
            serde_json::from_str(include_str!("../../docs/fixtures/task-checklist-v1.json"))
                .unwrap();
        #[derive(serde::Deserialize)]
        struct InvalidCase {
            id: String,
            kind: String,
            document: Box<serde_json::value::RawValue>,
        }
        let invalid: Vec<InvalidCase> = serde_json::from_str(raw["invalid"].get()).unwrap();
        for c in invalid.iter().filter(|c| c.kind == kind) {
            let rejected = if kind == "items" {
                parse_items(c.document.get()).is_err()
            } else {
                parse_definitions(c.document.get()).is_err()
            };
            assert!(rejected, "{}", c.id);
        }
        for c in f["merges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == kind)
        {
            for (a, b) in [(&c["left"], &c["right"]), (&c["right"], &c["left"])] {
                let result = if kind == "items" {
                    merge_items(&item_doc(a), &item_doc(b)).and_then(|d| encode_items(&d))
                } else {
                    merge_definitions(
                        &parse_definitions(&a.to_string()).unwrap(),
                        &parse_definitions(&b.to_string()).unwrap(),
                    )
                    .and_then(|d| encode_definitions(&d))
                };
                if c["error"].is_string() || c["error"] == true {
                    assert!(result.is_err(), "{}", c["id"]);
                } else {
                    assert_eq!(
                        serde_json::from_str::<Value>(&result.unwrap()).unwrap(),
                        parse(&c["expected"]).unwrap(),
                        "{}",
                        c["id"]
                    );
                }
            }
        }
    }
    assert!(parse_items(&" ".repeat(4194305)).is_err());
}
const TODO: &str = "123e4567-e89b-42d3-a456-000000000002";
const BY: &str = "123e4567-e89b-42d3-a456-000000000003";
fn parent(db: &Connection) {
    db.execute("INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,updated_by,due_date,reminder_at) VALUES(?1,'original','note',0,1,1,?2,'2026-09-20',5000)",[TODO,BY]).unwrap();
}
#[test]
fn checklist_panel_snapshot_progress_and_tombstones() {
    let mut db = db();
    parent(&db);
    let empty = crate::task_checklist_views::read(&mut db, TODO).unwrap();
    assert_eq!(empty.title, "original");
    assert!(!empty.read_only);
    assert!(crate::task_checklist_views::progress(&mut db)
        .unwrap()
        .is_empty());
    let mut r = request();
    r.items[0].completed = true;
    store::save(&mut db, &r, 10, BY).unwrap();
    let counts = crate::task_checklist_views::progress(&mut db).unwrap();
    assert_eq!((counts[0].total, counts[0].completed), (1, 1));
    assert_eq!(
        db.query_row("SELECT completed FROM todos WHERE uuid=?1", [TODO], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    db.execute("UPDATE todos SET archived_at=20 WHERE uuid=?1", [TODO])
        .unwrap();
    assert!(
        crate::task_checklist_views::read(&mut db, TODO)
            .unwrap()
            .read_only
    );
    db.execute(
        "UPDATE todos SET archived_at=NULL,deleted_at=20 WHERE uuid=?1",
        [TODO],
    )
    .unwrap();
    assert!(crate::task_checklist_views::read(&mut db, TODO)
        .unwrap_err()
        .contains("MISSING"));
    assert!(crate::task_checklist_views::progress(&mut db)
        .unwrap()
        .is_empty());
    db.execute("UPDATE todos SET deleted_at=NULL WHERE uuid=?1", [TODO])
        .unwrap();
    r.operation_uuid = uuid::Uuid::new_v4().to_string();
    r.expected_updated_at = 10;
    r.expected_items = crate::task_checklist_views::read(&mut db, TODO)
        .unwrap()
        .items;
    r.items.clear();
    store::save(&mut db, &r, 30, BY).unwrap();
    assert!(crate::task_checklist_views::progress(&mut db)
        .unwrap()
        .is_empty());
    let snapshot = crate::task_checklist_views::read(&mut db, TODO).unwrap();
    assert_eq!(snapshot.items.items.len(), 1);
    assert_eq!(snapshot.items.items[0].deleted_at, Some(30));
}
fn request() -> store::ChecklistSave {
    store::ChecklistSave {
        operation_uuid: uuid::Uuid::new_v4().to_string(),
        todo_uuid: TODO.into(),
        expected_updated_at: 1,
        expected_items: ItemsDocument::default(),
        title: "edited".into(),
        note: "body".into(),
        items: vec![store::ChecklistEdit {
            uuid: uuid::Uuid::new_v4().to_string(),
            content: "step".into(),
            sort_order: 0,
            completed: false,
        }],
    }
}
#[test]
fn atomic_save_retry_conflict_visibility() {
    let mut c = db();
    parent(&c);
    let r = request();
    c.execute_batch("CREATE TRIGGER fail_child BEFORE INSERT ON task_checklist_items BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(store::save(&mut c, &r, 10, BY)
        .unwrap_err()
        .contains("injected"));
    assert_eq!(
        c.query_row("SELECT title FROM todos", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    assert!(store::snapshot(&mut c).unwrap().items.items.is_empty());
    c.execute_batch("DROP TRIGGER fail_child").unwrap();
    assert_eq!(store::save(&mut c, &r, 10, BY).unwrap(), 10);
    let saved = store::snapshot(&mut c).unwrap();
    assert_eq!(store::save(&mut c, &r, 100, BY).unwrap(), 10);
    assert_eq!(store::snapshot(&mut c).unwrap(), saved);
    let mut bad = r.clone();
    bad.title = "different".into();
    assert!(store::save(&mut c, &bad, 20, BY)
        .unwrap_err()
        .contains("OPERATION_REUSED"));
    bad.operation_uuid = uuid::Uuid::new_v4().to_string();
    assert!(store::save(&mut c, &bad, 20, BY)
        .unwrap_err()
        .contains("STALE"));
    assert_eq!(
        c.query_row("SELECT reminder_at FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
    c.execute_batch("UPDATE todos SET deleted_at=20").unwrap();
    assert!(store::visible_items(&mut c, TODO).unwrap().is_empty());
    c.execute_batch("UPDATE todos SET deleted_at=NULL").unwrap();
    assert_eq!(store::visible_items(&mut c, TODO).unwrap().len(), 1);
    bad.expected_updated_at = 10;
    bad.expected_items = saved.items;
    c.execute_batch("UPDATE todos SET archived_at=20").unwrap();
    assert!(store::save(&mut c, &bad, 20, BY)
        .unwrap_err()
        .contains("READ_ONLY"));
    c.execute_batch("UPDATE todos SET archived_at=NULL")
        .unwrap();
    bad.items.clear();
    assert_eq!(store::save(&mut c, &bad, 20, BY).unwrap(), 20);
    assert!(store::visible_items(&mut c, TODO).unwrap().is_empty());
    let deleted = store::snapshot(&mut c).unwrap();
    assert_eq!(deleted.items.items[0].deleted_at, Some(20));
    store::merge(&mut c, &r.expected_items, &DefinitionsDocument::default()).unwrap();
    assert_eq!(store::snapshot(&mut c).unwrap(), deleted);
    let mut reuse = bad.clone();
    reuse.operation_uuid = uuid::Uuid::new_v4().to_string();
    reuse.expected_updated_at = 20;
    reuse.expected_items = deleted.items;
    reuse.items = r.items;
    assert!(store::save(&mut c, &reuse, 30, BY)
        .unwrap_err()
        .contains("ID_REUSED"));
}
#[test]
fn merge_idempotence_and_two_domain_rollback() {
    let mut c = db();
    let f = fixtures();
    let items = item_doc(&json!({"format_version":1,"items":[f["base_item"].clone()]}));
    let defs = parse_definitions(
        &json!({"format_version":1,"definitions":[f["base_definition"].clone()]}).to_string(),
    )
    .unwrap();
    c.execute_batch("CREATE TRIGGER fail_def BEFORE INSERT ON task_checklist_definitions BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(store::merge(&mut c, &items, &defs).is_err());
    assert_eq!(store::snapshot(&mut c).unwrap().item_state.revision, 0);
    c.execute_batch("DROP TRIGGER fail_def").unwrap();
    let s = store::merge(&mut c, &items, &defs).unwrap();
    assert_eq!(s.item_state.revision, 1);
    assert_eq!(s.definition_state.revision, 1);
    assert_eq!(store::merge(&mut c, &items, &defs).unwrap(), s);
    c.execute_batch("UPDATE task_checklist_items SET todo_uuid='corrupt'")
        .unwrap();
    assert!(store::snapshot(&mut c).is_err());
}
#[test]
fn migration_upgrade_idempotence_and_rollback() {
    let mut c = db();
    parent(&c);
    crate::db::migrate(&mut c).unwrap();
    c.execute_batch("DROP TABLE task_checklist_items; DROP TABLE task_checklist_definitions; DROP TABLE task_checklist_sync_state; DROP TABLE task_checklist_operations; DELETE FROM schema_migrations WHERE version=20;").unwrap();
    // Abort the version marker after DDL to prove the entire migration rolls back.
    c.execute_batch("CREATE TRIGGER fail_version BEFORE INSERT ON schema_migrations WHEN NEW.version=20 BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(crate::db::migrate(&mut c).is_err());
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='task_checklist_items'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER fail_version").unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(
        c.query_row("SELECT title FROM todos", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    assert!(store::snapshot(&mut c).unwrap().items.items.is_empty());
}

#[test]
fn receipt_failure_and_remote_child_edits_are_atomic() {
    let mut c = db();
    parent(&c);
    let r = request();
    c.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON task_checklist_operations BEGIN SELECT RAISE(ABORT,'receipt failure'); END;").unwrap();
    assert!(store::save(&mut c, &r, 10, BY).is_err());
    assert_eq!(
        c.query_row("SELECT updated_at FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(store::snapshot(&mut c).unwrap().items.items.is_empty());
    c.execute_batch("DROP TRIGGER fail_receipt").unwrap();
    let remote = item_doc(&json!({"format_version":1,"items":[fixtures()["base_item"].clone()]}));
    store::merge(&mut c, &remote, &DefinitionsDocument::default()).unwrap();
    assert!(store::save(&mut c, &r, 10, BY)
        .unwrap_err()
        .contains("STALE"));
    assert_eq!(
        c.query_row("SELECT count(*) FROM task_checklist_operations", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
}

#[test]
fn local_limit_remote_oversize_and_noop() {
    let mut c = db();
    parent(&c);
    let mut r = request();
    r.items = (0..21)
        .map(|n| store::ChecklistEdit {
            uuid: uuid::Uuid::new_v4().to_string(),
            content: format!("step {n}"),
            sort_order: n,
            completed: false,
        })
        .collect();
    assert!(store::save(&mut c, &r, 10, BY)
        .unwrap_err()
        .contains("LOCAL_LIMIT"));
    let base: ChecklistItem = serde_json::from_value(fixtures()["base_item"].clone()).unwrap();
    let remote = ItemsDocument {
        format_version: 1,
        items: r
            .items
            .iter()
            .map(|e| ChecklistItem {
                uuid: e.uuid.clone(),
                content: e.content.clone(),
                sort_order: e.sort_order,
                ..base.clone()
            })
            .collect(),
    };
    let before = store::merge(&mut c, &remote, &DefinitionsDocument::default()).unwrap();
    r.expected_items = remote;
    r.title = "original".into();
    r.note = "note".into();
    assert_eq!(store::save(&mut c, &r, 3000, BY).unwrap(), 1);
    assert_eq!(store::snapshot(&mut c).unwrap(), before);
    r.operation_uuid = uuid::Uuid::new_v4().to_string();
    r.items[0].completed = true;
    assert_eq!(store::save(&mut c, &r, 3000, BY).unwrap(), 3000);
    let after = store::snapshot(&mut c).unwrap();
    assert_eq!(after.items.items.len(), 21);
    assert_eq!(after.item_state.revision, before.item_state.revision + 1);
}

#[test]
fn caller_owned_create_transaction_and_exhausted_revision() {
    let mut c = db();
    let r = request();
    {
        let tx = c.transaction().unwrap();
        parent(&tx);
        assert_eq!(store::save_in_transaction(&tx, &r, 10, BY).unwrap(), 10);
        // An outer creation failure also rolls back children, dirty state and the receipt.
        tx.rollback().unwrap();
    }
    assert_eq!(
        c.query_row("SELECT count(*) FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(store::snapshot(&mut c).unwrap().item_state.revision, 0);
    parent(&c);
    c.execute(
        "UPDATE task_checklist_sync_state SET revision=?1 WHERE domain='items'",
        [MAX_CLOCK],
    )
    .unwrap();
    assert!(store::save(&mut c, &r, 10, BY).is_err());
    assert_eq!(
        c.query_row("SELECT title FROM todos", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    assert!(store::snapshot(&mut c).unwrap().items.items.is_empty());
}

#[test]
fn utf16_limits_and_clock_guards() {
    let mut i: ChecklistItem = serde_json::from_value(fixtures()["base_item"].clone()).unwrap();
    i.content = "\u{1f600}".repeat(100);
    assert!(validate_item(&i).is_ok());
    i.content.push('x');
    assert!(validate_item(&i).is_err());
    i.content = "\u{feff}trim".into();
    assert!(validate_item(&i).is_err());
    let mut c = db();
    parent(&c);
    let mut r = request();
    c.execute("UPDATE todos SET updated_at=?1", [MAX_CLOCK])
        .unwrap();
    r.expected_updated_at = MAX_CLOCK;
    assert!(store::save(&mut c, &r, MAX_CLOCK, BY)
        .unwrap_err()
        .contains("CLOCK_EXHAUSTED"));
}

#[test]
fn checklist_survives_database_reopen() {
    let path =
        std::env::temp_dir().join(format!("eggdone-checklist-{}.sqlite", uuid::Uuid::new_v4()));
    let r = request();
    {
        let mut c = Connection::open(&path).unwrap();
        crate::db::migrate(&mut c).unwrap();
        parent(&c);
        store::save(&mut c, &r, 10, BY).unwrap();
    }
    {
        let mut c = Connection::open(&path).unwrap();
        crate::db::migrate(&mut c).unwrap();
        assert_eq!(
            store::visible_items(&mut c, TODO).unwrap()[0].content,
            "step"
        );
        assert_eq!(store::save(&mut c, &r, 30, BY).unwrap(), 10);
        assert_eq!(store::snapshot(&mut c).unwrap().item_state.revision, 1);
    }
    std::fs::remove_file(path).unwrap();
}
