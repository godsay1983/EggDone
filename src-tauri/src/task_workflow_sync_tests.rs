use super::*;
use crate::{
    db::Database,
    recurrence_transport::tests::{Reply, Server},
    s3_sync::PreparedManualSync,
    task_workflow_session,
};
use std::sync::{Arc, Mutex};

const TASK: &str = "123e4567-e89b-42d3-a456-426614174000";
const EMPTY: &str = r#"{"format_version":1,"events":[],"states":[]}"#;

#[test]
fn workflow_conflict_restarts_entities_with_two_attempt_bound() {
    tauri::async_runtime::block_on(async {
        for conflicts in [1, 2] {
            let db = Database {
                connection: Mutex::new(db()),
            };
            let mut replies = vec![];
            for attempt in 0..2 {
                replies.extend([
                    Reply::new(404, None, b""), // todos
                    Reply::new(404, None, b""), // rules
                    Reply::new(200, None, b""), // todos upload
                    Reply::new(404, None, b""), // definitions
                    Reply::new(404, None, b""), // items
                    Reply::new(404, None, b""), // templates
                    Reply::new(404, None, b""), // plans
                    Reply::new(200, Some("\"old\""), document(1).as_bytes()),
                    Reply::new(
                        if attempt < conflicts { 412 } else { 200 },
                        Some("\"new\""),
                        b"",
                    ),
                ]);
            }
            if conflicts == 1 {
                replies.extend([
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
                assert!(matches!(result,Err(e) if e=="TASK_NOTE_LINK_SYNC_CONFLICT"));
            }
            for _ in 0..2 {
                for method in [
                    "GET", "GET", "PUT", "GET", "GET", "GET", "GET", "GET", "PUT",
                ] {
                    assert!(server.request().head.starts_with(method));
                }
            }
            let s = state(&mut db.connection.lock().unwrap());
            assert_eq!(s.revision == s.synced_revision, conflicts == 1);
        }
    });
}

#[test]
fn workflow_event_after_plan_ack_restarts_and_returns_current_receipts() {
    tauri::async_runtime::block_on(async {
        let db = Database {
            connection: Mutex::new(db()),
        };
        let raw=serde_json::json!({"format_version":1,"events":[{"task_uuid":TASK,"event_id":"1:61:0:-:-"}],"states":[]}).to_string();
        let mut replies = vec![];
        for attempt in 0..2 {
            replies.extend([
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(200, None, b""),
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
            ]);
            if attempt == 1 {
                replies.push(Reply::new(200, Some("\"plans\""), b""));
            }
            replies.extend([
                Reply::new(200, Some("\"workflow\""), raw.as_bytes()),
                Reply::new(200, Some("\"workflow\""), b""),
            ]);
        }
        replies.extend([
            Reply::new(404, None, b""),
            Reply::new(200, None, b""),
            Reply::new(404, None, b""),
        ]);
        let server = Server::new(replies);
        let p =
            PreparedManualSync::from_test_bucket(&db.connection.lock().unwrap(), server.bucket());
        let result = crate::task_note_link_session::run(&db, &p).await.unwrap();
        assert!(result.conflict_retried);
        assert_eq!(
            crate::daily_plan_session::final_token(&db, &result.plan_receipt)
                .unwrap()
                .as_deref(),
            Some("etag:\"plans\"")
        );
        assert_eq!(
            crate::task_workflow_session::final_token(&db, &result.workflow_receipt)
                .unwrap()
                .as_deref(),
            Some("etag:\"workflow\"")
        );
        let mut c = db.connection.lock().unwrap();
        let workflow = store::snapshot(&mut c).unwrap();
        let planning = crate::daily_plan_store::snapshot(&mut c).unwrap();
        assert_eq!(workflow.document.events, planning.document.events);
        assert_eq!(workflow.revision, workflow.synced_revision);
        assert_eq!(planning.revision, planning.synced_revision);
    });
}

fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn document(clock: i64) -> String {
    serde_json::json!({"format_version":1,"events":[],"states":[{
        "task_uuid":TASK,"state":"waiting","reason":"private","review_date":null,
        "clock":clock,"writer":"desktop","basis":""
    }]})
    .to_string()
}
fn merge(db: &mut Connection, raw: &str) {
    let tx = db.transaction().unwrap();
    store::merge_in_transaction(&tx, &protocol::parse(raw).unwrap()).unwrap();
    tx.commit().unwrap();
}
fn state(db: &mut Connection) -> store::Snapshot {
    let tx = db.transaction().unwrap();
    store::read_in_transaction(&tx).unwrap()
}

#[test]
fn sidecar_hashes_the_entire_utf8_key_and_checks_collisions() {
    let key = object_key("account/todos.json", &[]).unwrap();
    assert_eq!(
        key,
        format!(
            "eggdone-workflow/v1/{:x}/states.json",
            Sha256::digest(b"account/todos.json")
        )
    );
    assert_ne!(key, object_key("other/todos.json", &[]).unwrap());
    assert_ne!(key, object_key("account/other.json", &[]).unwrap());
    assert_eq!(
        object_key("account/todos.json", &[key]).unwrap_err(),
        "WORKFLOW_KEY_COLLISION"
    );
    assert!(object_key("../todos.json", &[]).is_err());
    let todo = "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/todos.json";
    assert!(crate::sync_space::scope(&object_key(todo, &[]).unwrap())
        .unwrap()
        .is_none());
}

#[test]
fn receipts_require_exact_revision_generation_and_epoch() {
    let mut db = db();
    let epoch = sync_target::capture(&db).unwrap();
    merge(&mut db, &document(1));
    let captured = prepare(&mut db, &epoch, None, None).unwrap();
    assert!(acknowledge(&mut db, &captured, None).is_err());
    assert!(acknowledge(&mut db, &captured, Some("W/\"weak\"")).is_err());
    merge(&mut db, &document(2));
    assert!(!acknowledge(&mut db, &captured, Some("\"created\"")).unwrap());
    let s = state(&mut db);
    assert!(s.revision > s.synced_revision);
    assert_eq!(s.etag.as_deref(), Some("\"created\""));
    assert_eq!(
        prepare(&mut db, &epoch, None, None).unwrap_err(),
        "WORKFLOW_REMOTE_MISSING"
    );
    let captured = prepare(&mut db, &epoch, Some(EMPTY), Some("\"remote\"")).unwrap();
    db.execute(
        "UPDATE task_workflow_sync_state SET generation=generation+1",
        [],
    )
    .unwrap();
    assert!(!acknowledge(&mut db, &captured, Some("\"stale-generation\"")).unwrap());
    let captured = prepare(&mut db, &epoch, Some(EMPTY), Some("\"remote\"")).unwrap();
    sync_target::invalidate(&db).unwrap();
    sync_target::activate(&db).unwrap();
    assert!(!acknowledge(&mut db, &captured, Some("\"stale-target\"")).unwrap());
    assert!(prepare(&mut db, &epoch, None, None).is_err());
    assert!(state(&mut db).etag.is_none());
    assert!(crate::sync_runtime_state::get_snapshot(&db)
        .unwrap()
        .dirty_domains
        .contains(&"workflow".into()));
}

#[test]
fn observed_remote_disappearance_is_rejected_without_ack_or_merge() {
    let mut db = db();
    let epoch = sync_target::capture(&db).unwrap();
    let captured = prepare(&mut db, &epoch, Some(&document(1)), Some("\"seen\"")).unwrap();
    assert_eq!(
        prepare(&mut db, &epoch, None, None).unwrap_err(),
        "WORKFLOW_REMOTE_MISSING"
    );
    assert!(is_current(&mut db, &captured).unwrap());
    assert_eq!(state(&mut db).synced_revision, 0);
    assert!(acknowledge(&mut db, &captured, Some("\"uploaded\"")).unwrap());
    assert!(!crate::sync_runtime_state::get_snapshot(&db)
        .unwrap()
        .dirty_domains
        .contains(&"workflow".into()));
}

#[test]
fn failed_ack_rolls_back_all_receipt_fields() {
    let mut db = db();
    let epoch = sync_target::capture(&db).unwrap();
    merge(&mut db, &document(1));
    let captured = prepare(&mut db, &epoch, None, None).unwrap();
    db.execute_batch("CREATE TRIGGER fail_plan_ack BEFORE UPDATE OF synced_revision ON task_workflow_sync_state BEGIN SELECT RAISE(ABORT,'ack'); END;").unwrap();
    assert!(acknowledge(&mut db, &captured, Some("\"uploaded\"")).is_err());
    let s = state(&mut db);
    assert_eq!(s.synced_revision, 0);
    assert!(s.etag.is_none());
}

#[test]
fn workflow_http_discovers_remote_on_empty_local_and_uses_conditional_writes() {
    tauri::async_runtime::block_on(async {
        for exists in [false, true] {
            let db = Database {
                connection: Mutex::new(db()),
            };
            let mut replies = vec![if exists {
                Reply::new(200, Some("\"old\""), document(1).as_bytes())
            } else {
                Reply::new(404, None, b"")
            }];
            if exists {
                replies.push(Reply::new(200, Some("\"new\""), b""));
            }
            let server = Server::new(replies);
            let prepared = PreparedManualSync::from_test_bucket(
                &db.connection.lock().unwrap(),
                server.bucket(),
            );
            let receipt = task_workflow_session::attempt(&db, &prepared)
                .await
                .unwrap()
                .unwrap();
            let key = object_key("account/todos.json", &[]).unwrap();
            assert!(server
                .request()
                .head
                .starts_with(&format!("GET /rules-test/{key} ")));
            if exists {
                let upload = server.request();
                assert!(upload.head.to_lowercase().contains("if-match: \"old\""));
                assert_eq!(
                    protocol::parse(std::str::from_utf8(&upload.body).unwrap())
                        .unwrap()
                        .states
                        .len(),
                    1
                );
            }
            assert_eq!(
                task_workflow_session::final_token(&db, &receipt)
                    .unwrap()
                    .as_deref(),
                Some(if exists { "etag:\"new\"" } else { "missing" })
            );
            merge(&mut db.connection.lock().unwrap(), &document(2));
            assert!(task_workflow_session::final_token(&db, &receipt)
                .unwrap()
                .is_none());
            let s = state(&mut db.connection.lock().unwrap());
            assert!(s.revision > s.synced_revision);
        }
    });
}

#[test]
fn workflow_http_conflict_failure_late_edit_and_epoch_switch_never_ack() {
    tauri::async_runtime::block_on(async {
        for mode in ["conflict", "denied", "late-edit", "target"] {
            let db = Arc::new(Database {
                connection: Mutex::new(db()),
            });
            merge(&mut db.connection.lock().unwrap(), &document(1));
            let captured = db.clone();
            let status = match mode {
                "conflict" => 412,
                "denied" => 403,
                _ => 200,
            };
            let server = Server::new(vec![
                Reply::new(404, None, b""),
                Reply::new(status, Some("\"created\""), b"").with_hook(move || {
                    let mut connection = captured.connection.lock().unwrap();
                    if mode == "late-edit" {
                        merge(&mut connection, &document(2));
                    }
                    if mode == "target" {
                        sync_target::invalidate(&connection).unwrap();
                        sync_target::activate(&connection).unwrap();
                    }
                }),
            ]);
            let prepared = PreparedManualSync::from_test_bucket(
                &db.connection.lock().unwrap(),
                server.bucket(),
            );
            let result = task_workflow_session::attempt(&db, &prepared).await;
            match mode {
                "conflict" | "late-edit" => assert!(result.unwrap().is_none()),
                _ => assert!(result.is_err()),
            }
            server.request();
            assert!(server
                .request()
                .head
                .to_lowercase()
                .contains("if-none-match: *"));
            let s = state(&mut db.connection.lock().unwrap());
            assert_eq!(s.synced_revision, 0);
            assert!(s.revision > s.synced_revision);
        }
    });
}

#[test]
fn workflow_http_validates_missing_etags_corruption_and_remote_disappearance() {
    tauri::async_runtime::block_on(async {
        for (status, etag, body) in [
            (200, None, EMPTY),
            (200, Some("W/\"weak\""), EMPTY),
            (200, Some("\"e\""), "{}"),
            (403, None, ""),
            (503, None, ""),
        ] {
            let db = Database {
                connection: Mutex::new(db()),
            };
            let server = Server::new(vec![Reply::new(status, etag, body.as_bytes())]);
            let prepared = PreparedManualSync::from_test_bucket(
                &db.connection.lock().unwrap(),
                server.bucket(),
            );
            assert!(task_workflow_session::attempt(&db, &prepared)
                .await
                .is_err());
            assert!(state(&mut db.connection.lock().unwrap()).etag.is_none());
        }
        let db = Database {
            connection: Mutex::new(db()),
        };
        let server = Server::new(vec![
            Reply::new(200, Some("\"seen\""), EMPTY.as_bytes()),
            Reply::new(412, None, b""),
            Reply::new(404, None, b""),
        ]);
        let prepared =
            PreparedManualSync::from_test_bucket(&db.connection.lock().unwrap(), server.bucket());
        assert!(task_workflow_session::attempt(&db, &prepared)
            .await
            .unwrap()
            .is_none());
        assert!(
            matches!(task_workflow_session::attempt(&db, &prepared).await, Err(error) if error == "WORKFLOW_REMOTE_MISSING")
        );
    });
}
