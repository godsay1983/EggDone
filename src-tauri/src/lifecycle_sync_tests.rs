use super::*;
const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
const OP: &str = "123e4567-e89b-42d3-a456-426614174090";
const MAIN: &str = "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/todos.json";
fn document() -> Document {
    Document {
        format_version: 1,
        terminals: vec![
            Terminal {
                kind: "todo".into(),
                uuid: TODO.into(),
                operation_uuid: OP.into(),
                purged_at: 200,
            },
            Terminal {
                kind: "note".into(),
                uuid: NOTE.into(),
                operation_uuid: OP.into(),
                purged_at: 200,
            },
        ],
    }
}
fn seed() -> (Connection, String) {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/trash-restore-v1.json")).unwrap();
    for sql in fixture["seed"].as_array().unwrap() {
        c.execute_batch(sql.as_str().unwrap()).unwrap();
    }
    c.execute(
        "UPDATE note_attachments SET remote_uploaded=1,sha256=?1",
        ["a".repeat(64)],
    )
    .unwrap();
    for table in ["todos", "notes", "note_attachments"] {
        c.execute(&format!("UPDATE {table} SET updated_by=?1"), [OP])
            .unwrap();
    }
    c.execute(
        "UPDATE todos SET repeat_rule=NULL,repeat_series_uuid=NULL",
        [],
    )
    .unwrap();
    c.execute("UPDATE sync_settings SET enabled=1,object_key=?1", [MAIN])
        .unwrap();
    let epoch = crate::sync_target::capture(&c).unwrap();
    (c, epoch)
}
#[test]
fn shared_validation_and_union_algebra() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/lifecycle-sync-v1.json")).unwrap();
    for case in cases.as_array().unwrap() {
        assert_eq!(
            parse(case["raw"].as_str().unwrap()).is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    let a = document();
    let mut b = a.clone();
    b.terminals[0].purged_at = 1000;
    let mut c = a.clone();
    c.terminals[1].operation_uuid = "123e4567-e89b-42d3-a456-426614174099".into();
    assert_eq!(merge(&a, &b).unwrap(), merge(&b, &a).unwrap());
    assert_eq!(
        merge(&merge(&a, &b).unwrap(), &c).unwrap(),
        merge(&a, &merge(&b, &c).unwrap()).unwrap()
    );
    let merged = merge(&a, &b).unwrap();
    assert_eq!(merge(&merged, &merged).unwrap(), merged);
}
#[test]
fn atomic_apply_filters_stale_bodies_and_preserves_asset_evidence() {
    let (mut c, epoch) = seed();
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    value["base_rule"]["first_todo_uuid"] = TODO.into();
    value["base_rule"]["current_todo_uuid"] = TODO.into();
    value["base_rule"]["generated_count"] = 1.into();
    let rules = crate::recurrence_protocol::parse_document(
        &serde_json::json!({"format_version":1,"rules":[value["base_rule"]]}).to_string(),
    )
    .unwrap();
    crate::recurrence_store::merge(&mut c, &rules).unwrap();
    let raw: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(include_str!("../../docs/fixtures/task-checklist-v1.json")).unwrap();
    let fixture: serde_json::Value = serde_json::json!({"base_item":serde_json::from_str::<serde_json::Value>(raw["base_item"].get()).unwrap(),"base_definition":serde_json::from_str::<serde_json::Value>(raw["base_definition"].get()).unwrap()});
    let mut item = fixture["base_item"].clone();
    item["todo_uuid"] = TODO.into();
    let items = crate::task_checklist_protocol::parse_items(
        &serde_json::json!({"format_version":1,"items":[item]}).to_string(),
    )
    .unwrap();
    let definitions = crate::task_checklist_protocol::parse_definitions(
        &serde_json::json!({"format_version":1,"definitions":[fixture["base_definition"]]})
            .to_string(),
    )
    .unwrap();
    crate::task_checklist_store::merge(&mut c, &items, &definitions).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/task-note-links-v1.json")).unwrap();
    let links = crate::task_note_link_protocol::parse_document(
        &serde_json::json!({"format_version":1,"links":[fixture["base_record"]]}).to_string(),
    )
    .unwrap();
    crate::task_note_link_store::merge(&mut c, &links).unwrap();
    c.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'unrelated',0,1,1,?1)", [OP]).unwrap();
    let mut todos = crate::sync::build_document(&c, 300).unwrap();
    let mut notes = crate::note_sync::build_document(&c, 300).unwrap();
    let mut attachments = crate::note_attachment_sync::build_document(&c, 300).unwrap();
    for t in &mut todos.todos {
        t.deleted_at = None;
        t.updated_at = 10000;
    }
    for n in &mut notes.notes {
        n.deleted_at = None;
        n.updated_at = 10000;
    }
    let mut unrelated_note = notes.notes[0].clone();
    unrelated_note.uuid = OP.into();
    notes.notes.push(unrelated_note);
    let mut unrelated_asset = attachments.attachments[0].clone();
    unrelated_asset.uuid = "123e4567-e89b-42d3-a456-426614174091".into();
    unrelated_asset.note_uuid = OP.into();
    unrelated_asset.deleted_at = None;
    attachments.attachments.push(unrelated_asset);
    let snapshot = prepare(&mut c, &epoch, &document()).unwrap();
    assert_eq!(snapshot.document.terminals.len(), 2);
    assert!(crate::recurrence_store::merge(&mut c, &rules)
        .unwrap()
        .document
        .rules
        .is_empty());
    let children = crate::task_checklist_store::merge(&mut c, &items, &definitions).unwrap();
    assert!(children.items.items.is_empty());
    assert_eq!(children.definitions.definitions.len(), 1);
    assert!(crate::task_note_link_store::merge(&mut c, &links)
        .unwrap()
        .document
        .links
        .is_empty());
    assert_eq!(
        prepare(&mut c, &epoch, &document()).unwrap().revision,
        snapshot.revision
    );
    assert_eq!(
        crate::sync::merge_remote_document(&mut c, &todos, 10001)
            .unwrap()
            .todos
            .len(),
        1
    );
    assert_eq!(
        crate::note_sync::merge_remote_document(&mut c, &notes, 10001)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.uuid.as_str())
            .collect::<Vec<_>>(),
        vec![OP]
    );
    assert_eq!(
        crate::note_attachment_sync::merge_remote_document(&mut c, &attachments, 10001)
            .unwrap()
            .attachments
            .iter()
            .map(|a| a.note_uuid.as_str())
            .collect::<Vec<_>>(),
        vec![OP]
    );
    let proofs: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'purge.remote.evidence.v1:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(proofs, 2);
    assert!(acknowledge(&mut c, &epoch, snapshot.revision, "\"ledger\"").unwrap());
    let mut next = document();
    next.terminals[0].purged_at += 1;
    prepare(&mut c, &epoch, &next).unwrap();
    assert!(!acknowledge(&mut c, &epoch, snapshot.revision, "\"old\"").unwrap());
    assert!(acknowledge(&mut c, "wrong", snapshot.revision, "\"old\"").is_err());
    assert!(
        crate::s3_sync::prepare_manual_sync(&c).is_err(),
        "normal entry remains closed"
    );
}
#[test]
fn rollback_and_legacy_scope_refusal() {
    let (mut c, epoch) = seed();
    c.execute_batch("CREATE TRIGGER fail_lifecycle BEFORE DELETE ON todos BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(prepare(&mut c, &epoch, &document()).is_err());
    assert!(purge::terminals(&c).unwrap().is_empty());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'purge.remote.evidence.v1:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER fail_lifecycle").unwrap();
    c.execute(
        "UPDATE sync_settings SET object_key='account/todos.json'",
        [],
    )
    .unwrap();
    assert!(matches!(prepare(&mut c,&epoch,&document()),Err(e) if e=="PURGE_MIGRATION_REQUIRED"));
}
#[test]
fn production_session_requires_readback_and_retries_cas() {
    use crate::{
        recurrence_transport::tests::{Reply, Server},
        s3_sync::PreparedManualSync,
    };
    for conflict in [false, true] {
        let (c, _) = seed();
        let key = MAIN.replace("todos.json", "lifecycle-terminals.json");
        let merged = merge(
            &Document {
                format_version: 1,
                terminals: vec![],
            },
            &document(),
        )
        .unwrap();
        let body = crate::sync_space::encode(&key, &encode(&merged).unwrap()).unwrap();
        let mut replies = vec![Reply::new(200, Some("\"one\""), body.as_bytes())];
        if conflict {
            replies.push(Reply::new(412, None, b""));
            replies.push(Reply::new(200, Some("\"two\""), body.as_bytes()));
        }
        replies.push(Reply::new(200, Some("\"done\""), b""));
        replies.push(Reply::new(200, Some("\"done\""), body.as_bytes()));
        let server = Server::new(replies);
        let prepared = PreparedManualSync::from_test_space(&c, server.bucket(), MAIN);
        let db = crate::db::Database {
            connection: std::sync::Mutex::new(c),
        };
        let token =
            tauri::async_runtime::block_on(crate::lifecycle_session::run(&db, &prepared)).unwrap();
        assert_eq!(token, "\"done\"");
        assert!(server.request().head.starts_with("GET "));
        let upload = server.request();
        assert!(upload.head.to_lowercase().contains("if-match: \"one\""));
        assert_eq!(std::str::from_utf8(&upload.body).unwrap(), body);
        let c = db.connection.lock().unwrap();
        assert_eq!(purge::terminals(&c).unwrap(), merged.terminals);
        let (revision, synced, etag): (i64, i64, Option<String>) = c
            .query_row(
                "SELECT revision,synced_revision,etag FROM lifecycle_sync_state WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(revision, synced);
        assert_eq!(etag.as_deref(), Some("\"done\""));
    }
    let (c, _) = seed();
    let server = Server::new(vec![Reply::new(404, None, b"")]);
    let prepared = PreparedManualSync::from_test_space(&c, server.bucket(), MAIN);
    let db = crate::db::Database {
        connection: std::sync::Mutex::new(c),
    };
    assert!(tauri::async_runtime::block_on(crate::lifecycle_session::run(&db, &prepared)).is_err());
    assert!(purge::terminals(&db.connection.lock().unwrap())
        .unwrap()
        .is_empty());
}

#[test]
fn readback_failure_never_acknowledges_and_retry_preserves_evidence() {
    use crate::{
        recurrence_transport::tests::{Reply, Server},
        s3_sync::PreparedManualSync,
    };
    let key = MAIN.replace("todos.json", "lifecycle-terminals.json");
    let body = crate::sync_space::encode(&key, &encode(&document()).unwrap()).unwrap();
    let empty = crate::sync_space::encode(
        &key,
        &encode(&Document {
            format_version: 1,
            terminals: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    for scenario in ["truncated", "upload-error", "corrupt", "weak-etag"] {
        let (c, _) = seed();
        let mut replies = vec![];
        if scenario == "truncated" {
            for _ in 0..3 {
                replies.extend([
                    Reply::new(200, Some("\"one\""), body.as_bytes()),
                    Reply::new(200, None, b""),
                    Reply::new(200, Some("\"bad\""), empty.as_bytes()),
                ]);
            }
        } else if scenario == "upload-error" {
            replies.extend([
                Reply::new(200, Some("\"one\""), body.as_bytes()),
                Reply::new(503, None, b""),
            ]);
        } else {
            replies.push(Reply::new(
                200,
                Some(if scenario == "weak-etag" {
                    "W/\"one\""
                } else {
                    "\"one\""
                }),
                if scenario == "corrupt" {
                    b"{}"
                } else {
                    body.as_bytes()
                },
            ));
        }
        let server = Server::new(replies);
        let prepared = PreparedManualSync::from_test_space(&c, server.bucket(), MAIN);
        let db = crate::db::Database {
            connection: std::sync::Mutex::new(c),
        };
        assert!(
            tauri::async_runtime::block_on(crate::lifecycle_session::run(&db, &prepared)).is_err(),
            "{scenario}"
        );
        {
            let c = db.connection.lock().unwrap();
            assert_eq!(
                c.query_row(
                    "SELECT etag FROM lifecycle_sync_state WHERE id=1",
                    [],
                    |r| r.get::<_, Option<String>>(0)
                )
                .unwrap(),
                None
            );
            assert_eq!(
                purge::terminals(&c).unwrap().len(),
                if ["truncated", "upload-error"].contains(&scenario) {
                    2
                } else {
                    0
                }
            );
        }
        if scenario == "upload-error" {
            let server = Server::new(vec![
                Reply::new(200, Some("\"retry\""), body.as_bytes()),
                Reply::new(200, None, b""),
                Reply::new(200, Some("\"done\""), body.as_bytes()),
            ]);
            let prepared = PreparedManualSync::from_test_space(
                &db.connection.lock().unwrap(),
                server.bucket(),
                MAIN,
            );
            assert_eq!(
                tauri::async_runtime::block_on(crate::lifecycle_session::run(&db, &prepared))
                    .unwrap(),
                "\"done\""
            );
        }
    }
}
