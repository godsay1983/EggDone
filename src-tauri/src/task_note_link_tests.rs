use crate::task_note_link_protocol::*;
use crate::task_note_link_store as store;
use rusqlite::Connection;
use serde_json::{json, Value};

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../docs/fixtures/task-note-links-v1.json")).unwrap()
}
fn document(value: &Value) -> LinkDocument {
    parse_document(&value.to_string()).unwrap()
}

#[test]
fn shared_protocol_fixtures() {
    let f = fixtures();
    for case in f["identities"].as_array().unwrap() {
        assert_eq!(
            link_uuid(
                case["todo"].as_str().unwrap(),
                case["note"].as_str().unwrap()
            )
            .unwrap(),
            case["expected"]
        );
    }
    for case in f["valid"].as_array().unwrap() {
        let doc = document(&case["document"]);
        assert_eq!(
            document(&serde_json::from_str::<Value>(&encode_document(&doc).unwrap()).unwrap()),
            merge_documents(&doc, &doc).unwrap()
        );
    }
    for case in f["invalid"].as_array().unwrap() {
        assert!(
            parse_document(&case["document"].to_string()).is_err(),
            "{}",
            case["id"]
        );
    }
    for case in f["merges"].as_array().unwrap() {
        let a = document(&case["left"]);
        let b = document(&case["right"]);
        let expected = document(&case["expected"]);
        assert_eq!(merge_documents(&a, &b).unwrap(), expected, "{}", case["id"]);
        assert_eq!(merge_documents(&b, &a).unwrap(), expected);
        assert_eq!(merge_documents(&expected, &b).unwrap(), expected);
    }
    for case in f["keys"].as_array().unwrap() {
        let occupied: Vec<String> = serde_json::from_value(case["occupied"].clone()).unwrap();
        let result = object_key(case["todo"].as_str().unwrap(), &occupied);
        if case["error"] == true {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), case["expected"]);
        }
    }
}

#[test]
fn limits_numbers_and_order_independence() {
    let f = fixtures();
    let mut doc = document(&f["valid"][1]["document"]);
    let text = encode_document(&doc)
        .unwrap()
        .replace("\"format_version\":1", "\"format_version\":1e0")
        .replace("\"updated_at\":2000", "\"updated_at\":2e3");
    assert_eq!(parse_document(&text).unwrap(), doc);
    assert!(parse_document(&" ".repeat(MAX_DOCUMENT_UNITS + 1)).is_err());
    assert!(parse_document(
        &json!({"format_version":1,"links":vec![doc.links[0].clone(); MAX_LINKS + 1]}).to_string()
    )
    .is_err());
    let base = doc.links[0].clone();
    for i in 0..21 {
        let mut link = base.clone();
        link.note_uuid = format!("123e4567-e89b-42d3-a456-{:012x}", i + 100);
        link.uuid = link_uuid(&link.todo_uuid, &link.note_uuid).unwrap();
        doc.links.push(link);
    }
    // The local creation limit must not truncate a valid remote union.
    assert_eq!(merge_documents(&doc, &doc).unwrap().links.len(), 22);
    let encoded = encode_document(&doc).unwrap();
    doc.links.reverse();
    assert_eq!(encode_document(&doc).unwrap(), encoded);
    let a = document(&f["merges"][0]["left"]);
    let b = document(&f["merges"][0]["right"]);
    let c = document(&f["merges"][1]["right"]);
    assert_eq!(
        merge_documents(&merge_documents(&a, &b).unwrap(), &c).unwrap(),
        merge_documents(&a, &merge_documents(&b, &c).unwrap()).unwrap()
    );
}

#[test]
fn repository_preserves_dangling_links_tombstones_and_atomic_revision() {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let f = fixtures();
    let active = document(&f["valid"][1]["document"]);
    assert_eq!(store::snapshot(&db).unwrap().revision, 0);
    assert_eq!(store::merge(&mut db, &active).unwrap().revision, 1);
    assert_eq!(store::merge(&mut db, &active).unwrap().revision, 1);
    assert_eq!(store::snapshot(&db).unwrap().document, active);
    let dead = document(&f["valid"][2]["document"]);
    store::merge(&mut db, &dead).unwrap();
    assert_eq!(store::merge(&mut db, &active).unwrap().document, dead);
    let before = store::snapshot(&db).unwrap();
    let mut batch = document(&f["valid"][3]["document"]);
    batch.links[0].updated_at = 5000;
    let second_id = batch.links[1].uuid.clone();
    db.execute_batch(&format!(
        "CREATE TRIGGER inject_failure BEFORE INSERT ON task_note_links
        WHEN NEW.uuid='{second_id}' BEGIN SELECT RAISE(ABORT,'injected failure'); END;"
    ))
    .unwrap();
    assert!(store::merge(&mut db, &batch).is_err());
    let after = store::snapshot(&db).unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.document, before.document);
    db.execute_batch("DROP TRIGGER inject_failure").unwrap();
    store::merge(&mut db, &batch).unwrap();
    assert_eq!(store::snapshot(&db).unwrap().document.links.len(), 2);
    assert_eq!(store::snapshot(&db).unwrap().synced_revision, 0);
    db.execute_batch("UPDATE task_note_link_sync_state SET revision=9007199254740991")
        .unwrap();
    batch.links[0].updated_at += 1;
    assert!(store::merge(&mut db, &batch).is_err());
    assert_eq!(store::snapshot(&db).unwrap().revision, MAX_CLOCK);
}

#[test]
fn migration_upgrade_failure_retry_and_disk_reopen() {
    let path = std::env::temp_dir().join(format!("eggdone-links-{}.sqlite", uuid::Uuid::new_v4()));
    let identity;
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        identity = crate::db::device_id(&db).unwrap();
        db.execute_batch(
            "DROP TABLE task_note_links; DROP TABLE task_note_link_sync_state;
            DELETE FROM schema_migrations WHERE version=18;
            INSERT INTO todos(uuid,title,completed,sort_order,created_at,updated_at,updated_by)
              VALUES('keep-task','keep task',0,0,1,1,'device');
            INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by)
              VALUES('keep-note','keep note','content','default',0,1,1,'device');",
        )
        .unwrap();
        let runtime: String = db
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
                r.get(0)
            })
            .unwrap();
        db.execute_batch(
            "CREATE TRIGGER fail_migration BEFORE INSERT ON schema_migrations
            WHEN NEW.version=18 BEGIN SELECT RAISE(ABORT,'migration failure'); END;",
        )
        .unwrap();
        assert!(crate::db::migrate(&mut db).is_err());
        assert_eq!(
            db.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            17
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='task_note_links'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        db.execute_batch("DROP TRIGGER fail_migration").unwrap();
        crate::db::migrate(&mut db).unwrap();
        crate::db::migrate(&mut db).unwrap();
        assert_eq!(
            db.query_row("SELECT title FROM todos", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep task"
        );
        assert_eq!(
            db.query_row("SELECT content FROM notes", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "content"
        );
        assert_eq!(
            db.query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            runtime
        );
        store::merge(&mut db, &document(&fixtures()["valid"][1]["document"])).unwrap();
    }
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        assert_eq!(store::snapshot(&db).unwrap().revision, 1);
        assert_eq!(crate::db::device_id(&db).unwrap(), identity);
    }
    std::fs::remove_file(path).unwrap();
}
