use crate::recurrence_transport::tests::{Reply, Server};
use crate::{
    db::{self, Database},
    s3_sync::PreparedManualSync,
    sync_target, task_checklist_protocol as p, task_checklist_store as store,
    task_checklist_sync::{self as snapshots, Domain},
    task_checklist_transport::*,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn documents() -> (p::ItemsDocument, p::DefinitionsDocument) {
    let f: Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-checklist-sync-v1.json"
    ))
    .unwrap();
    (
        p::parse_items(&json!({"format_version":1,"items":[f["base_item"].clone()]}).to_string())
            .unwrap(),
        p::parse_definitions(
            &json!({"format_version":1,"definitions":[f["base_definition"].clone()]}).to_string(),
        )
        .unwrap(),
    )
}
fn fixture() -> Database {
    let mut c = Connection::open_in_memory().unwrap();
    db::migrate(&mut c).unwrap();
    let by = db::device_id(&c).unwrap();
    let (items, defs) = documents();
    c.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'fixture',0,1,1,?2)",params![items.items[0].todo_uuid,by]).unwrap();
    store::merge(&mut c, &items, &defs).unwrap();
    Database {
        connection: Mutex::new(c),
    }
}
fn snapshot(db: &Database) -> snapshots::Snapshot {
    let mut c = db.connection.lock().unwrap();
    let epoch = sync_target::capture(&c).unwrap();
    snapshots::prepare(
        &mut c,
        &epoch,
        &Domain::Items.empty(),
        &Domain::Definitions.empty(),
        3000,
    )
    .unwrap()
}
fn state(db: &Database) -> store::Snapshot {
    store::snapshot(&mut db.connection.lock().unwrap()).unwrap()
}

#[test]
fn checklist_independent_ack_and_pending_summary() {
    let db = fixture();
    let s = snapshot(&db);
    assert!(snapshots::acknowledge(
        &mut db.connection.lock().unwrap(),
        &s,
        Domain::Definitions,
        Some("\"d\"")
    )
    .unwrap());
    let current = state(&db);
    assert_eq!(
        current.definition_state.revision,
        current.definition_state.synced_revision
    );
    assert!(current.item_state.revision > current.item_state.synced_revision);
    assert!(
        crate::sync_runtime_state::get_snapshot(&db.connection.lock().unwrap())
            .unwrap()
            .dirty_domains
            .contains(&"checklists".into())
    );
    assert!(snapshots::acknowledge(
        &mut db.connection.lock().unwrap(),
        &s,
        Domain::Items,
        Some("\"i\"")
    )
    .unwrap());
    assert!(
        !crate::sync_runtime_state::get_snapshot(&db.connection.lock().unwrap())
            .unwrap()
            .dirty_domains
            .contains(&"checklists".into())
    );
}

#[test]
fn checklist_stale_dependency_or_generation_never_acknowledged() {
    for sql in [
        "UPDATE todos SET title='changed'",
        "UPDATE sync_runtime_state SET todos_dirty_version=todos_dirty_version+1",
        "UPDATE recurrence_sync_state SET revision=revision+1",
        "UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='items'",
        "UPDATE task_checklist_sync_state SET generation=generation+1 WHERE domain='definitions'",
    ] {
        let db = fixture();
        let s = snapshot(&db);
        let mut c = db.connection.lock().unwrap();
        c.execute(sql, []).unwrap();
        assert!(!snapshots::is_current(&mut c, &s).unwrap(), "{sql}");
        assert!(!snapshots::acknowledge(&mut c, &s, Domain::Items, Some("\"late\"")).unwrap());
    }
}

#[test]
fn checklist_target_roundtrip_invalidates_both_generations_and_etags() {
    let db = fixture();
    let s = snapshot(&db);
    let mut c = db.connection.lock().unwrap();
    for _ in 0..2 {
        sync_target::invalidate(&c).unwrap();
        sync_target::activate(&c).unwrap();
    }
    assert!(!snapshots::acknowledge(&mut c, &s, Domain::Items, Some("\"late\"")).unwrap());
    let state = store::snapshot(&mut c).unwrap();
    assert_eq!(state.item_state.generation, 2);
    assert_eq!(state.definition_state.generation, 2);
    assert_eq!(state.item_state.etag, None);
    let epoch = sync_target::capture(&c).unwrap();
    c.execute(
        "UPDATE task_checklist_sync_state SET generation=9007199254740991 WHERE domain='items'",
        [],
    )
    .unwrap();
    assert!(sync_target::invalidate(&c).is_err());
    assert!(sync_target::is_current(&c, &epoch).unwrap());
}

#[test]
fn checklist_merge_is_atomic_and_replay_does_not_increment_revision() {
    let db = fixture();
    let s = snapshot(&db);
    let mut c = db.connection.lock().unwrap();
    let before = store::snapshot(&mut c).unwrap();
    let epoch = sync_target::capture(&c).unwrap();
    let (mut items, defs) = documents();
    items.items[0].content = "new".into();
    items.items[0].updated_at += 10;
    assert!(snapshots::prepare(
        &mut c,
        &epoch,
        &p::encode_items(&items).unwrap(),
        "invalid",
        3000
    )
    .is_err());
    assert_eq!(store::snapshot(&mut c).unwrap(), before);
    snapshots::prepare(
        &mut c,
        &epoch,
        &s.items,
        &p::encode_definitions(&defs).unwrap(),
        3000,
    )
    .unwrap();
    assert_eq!(store::snapshot(&mut c).unwrap(), before);
}

#[test]
fn checklist_signed_transport_conditions_scope_and_errors() {
    tauri::async_runtime::block_on(async {
        for domain in [Domain::Definitions, Domain::Items] {
            for existing in [false, true] {
                let body = domain.empty();
                let server = Server::new(vec![
                    Reply::new(
                        if existing { 200 } else { 404 },
                        Some("\"old\""),
                        body.as_bytes(),
                    ),
                    Reply::new(200, Some("\"new\""), b""),
                ]);
                let t = TaskChecklistTransport::new(
                    &server.bucket(),
                    "account/todos.json",
                    &[],
                    domain,
                )
                .unwrap();
                let remote = t.download().await.unwrap();
                let other = TaskChecklistTransport::new(
                    &server.bucket(),
                    "account/todos.json",
                    &[],
                    domain,
                )
                .unwrap();
                assert!(other.upload(&body, &remote).await.is_err());
                assert_eq!(
                    t.upload(&body, &remote).await.unwrap(),
                    ChecklistUploadOutcome::Uploaded {
                        etag: "\"new\"".into()
                    }
                );
                assert!(server.request().head.starts_with(&format!(
                    "GET /rules-test/account/task-checklist-{}.json ",
                    domain.name()
                )));
                let request = server.request();
                assert!(request.head.to_lowercase().contains(if existing {
                    "if-match: \"old\""
                } else {
                    "if-none-match: *"
                }));
                domain
                    .canonical(std::str::from_utf8(&request.body).unwrap())
                    .unwrap();
            }
            for (status, etag, body) in [
                (403, None, b"secret".as_slice()),
                (500, None, b"secret"),
                (200, None, b"{}"),
                (200, Some("W/\"weak\""), b"{}"),
                (200, Some("\"e\""), b"secret"),
                (200, Some("\"e\""), b"\xff"),
            ] {
                let server = Server::new(vec![Reply::new(status, etag, body)]);
                let t = TaskChecklistTransport::new(
                    &server.bucket(),
                    "account/todos.json",
                    &[],
                    domain,
                )
                .unwrap();
                let error = t.download().await.err().unwrap();
                assert!(!error.contains("secret"));
            }
        }
    });
}

#[test]
fn checklist_network_partial_failure_and_stale_put_leave_items_pending() {
    tauri::async_runtime::block_on(async {
        for late in [false, true] {
            let db = Arc::new(fixture());
            let captured = db.clone();
            let last = Reply::new(if late { 200 } else { 403 }, Some("\"i\""), b"secret")
                .with_hook(move || {
                    if late {
                        captured
                            .connection
                            .lock()
                            .unwrap()
                            .execute("UPDATE todos SET title='during put'", [])
                            .unwrap();
                    }
                });
            let server = Server::new(vec![
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(200, Some("\"d\""), b""),
                last,
            ]);
            let (prepared, todo) = {
                let c = db.connection.lock().unwrap();
                (
                    PreparedManualSync::from_test_bucket(&c, server.bucket()),
                    crate::sync::build_document(&c, 3000).unwrap(),
                )
            };
            let result = crate::task_checklist_session::attempt(&db, &prepared, &todo).await;
            if late {
                assert_eq!(result.unwrap(), None);
            } else {
                assert_eq!(result.unwrap_err(), "CHECKLIST_UPLOAD_HTTP:403");
            }
            let s = state(&db);
            assert_eq!(
                s.definition_state.revision,
                s.definition_state.synced_revision
            );
            assert!(s.item_state.revision > s.item_state.synced_revision);
        }
    });
}

#[test]
fn checklist_conflict_retries_parents_and_both_downloads_with_a_bound() {
    tauri::async_runtime::block_on(async {
        for conflicts in [1, 2] {
            let db = fixture();
            let mut replies = vec![];
            for attempt in 0..2 {
                replies.extend([
                    Reply::new(404, None, b""),
                    Reply::new(404, None, b""),
                    Reply::new(200, None, b""),
                    Reply::new(404, None, b""),
                    Reply::new(404, None, b""),
                    Reply::new(200, Some("\"d\""), b""),
                    Reply::new(
                        if attempt < conflicts { 412 } else { 200 },
                        Some("\"i\""),
                        b"",
                    ),
                ]);
            }
            if conflicts == 1 {
                replies.extend([
                    Reply::new(404, None, b""), // templates
                    Reply::new(404, None, b""),
                    Reply::new(200, None, b""),
                    Reply::new(404, None, b""),
                ]);
            }
            let server = Server::new(replies);
            let prepared = PreparedManualSync::from_test_bucket(
                &db.connection.lock().unwrap(),
                server.bucket(),
            );
            let result = crate::task_note_link_session::run(&db, &prepared).await;
            if conflicts == 1 {
                assert!(result.unwrap().conflict_retried);
            } else {
                assert!(result.is_err());
            }
            for _ in 0..2 {
                for (method, key) in [
                    ("GET", "todos.json"),
                    ("GET", "recurrence-rules.json"),
                    ("PUT", "todos.json"),
                    ("GET", "task-checklist-definitions.json"),
                    ("GET", "task-checklist-items.json"),
                    ("PUT", "task-checklist-definitions.json"),
                    ("PUT", "task-checklist-items.json"),
                ] {
                    assert!(server
                        .request()
                        .head
                        .starts_with(&format!("{method} /rules-test/account/{key} ")));
                }
            }
            assert_eq!(
                state(&db).item_state.revision == state(&db).item_state.synced_revision,
                conflicts == 1
            );
        }
    });
}

#[test]
#[ignore = "Only run through Harmony scripts/test-task-checklist-http.cjs with an isolated loopback peer"]
fn checklist_loopback_peer() {
    use s3::{creds::Credentials, Bucket, Region};
    let run = std::env::var("EGGDONE_NS7_S3_RUN").unwrap();
    assert_eq!(run.len(), 32);
    assert!(run.bytes().all(|c| c.is_ascii_hexdigit()));
    let port: u16 = std::env::var("EGGDONE_NS7_S3_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert!(port >= 1024);
    let phase = std::env::var("EGGDONE_CHECKLIST_PHASE").unwrap();
    assert!(["seed", "verify"].contains(&phase.as_str()));
    let bucket = Bucket::new(
        &format!("eggdone-ns7-{run}"),
        Region::Custom {
            region: "us-east-1".into(),
            endpoint: format!("http://127.0.0.1:{port}"),
        },
        Credentials::new(
            Some("eggdone-ns7-test-access"),
            Some("eggdone-ns7-public-test-fixture"),
            None,
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
    .with_path_style();
    let db = fixture();
    let prepared = PreparedManualSync::from_test_bucket(&db.connection.lock().unwrap(), bucket);
    let result =
        tauri::async_runtime::block_on(crate::task_note_link_session::run(&db, &prepared)).unwrap();
    assert!(result.checklist_token.is_some());
    let s = state(&db);
    if phase == "verify" {
        assert!(s.items.items[0].completed);
        assert_eq!(s.items.items[0].content, "edited on Harmony");
    }
    assert_eq!(s.item_state.revision, s.item_state.synced_revision);
    assert_eq!(
        s.definition_state.revision,
        s.definition_state.synced_revision
    );
    println!("CHECKLIST_DESKTOP_{}_OK", phase.to_uppercase());
}
