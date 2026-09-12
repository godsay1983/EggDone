use super::*;
use crate::recurrence_transport::tests::{Reply, Server};
use crate::{task_note_link_store as store, task_note_link_transport::TaskNoteLinkTransport};
use std::sync::{Arc, Mutex};
const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
fn fixture() -> Database {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let by = crate::db::device_id(&db).unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'todo',0,1,1,?2)",rusqlite::params![TODO,by]).unwrap();
    db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'note','body','default',0,1,1,?2)",rusqlite::params![NOTE,by]).unwrap();
    store::merge(
        &mut db,
        &protocol::LinkDocument {
            format_version: 1,
            links: vec![protocol::TaskNoteLink {
                uuid: protocol::link_uuid(TODO, NOTE).unwrap(),
                todo_uuid: TODO.into(),
                note_uuid: NOTE.into(),
                created_at: 1,
                updated_at: 1,
                updated_by: by,
                deleted_at: None,
            }],
        },
    )
    .unwrap();
    Database {
        connection: Mutex::new(db),
    }
}
fn entity_replies() -> Vec<Reply> {
    vec![
        Reply::new(404, None, b""),
        Reply::new(404, None, b""),
        Reply::new(200, None, b""),
        Reply::new(404, None, b""),
        Reply::new(200, None, b""),
    ]
}
fn prepared(db: &Database, server: &Server) -> PreparedManualSync {
    PreparedManualSync::from_test_bucket(&db.connection.lock().unwrap(), server.bucket())
}

#[test]
fn actual_signed_transport_entity_order_and_bounded_conflict_retry() {
    tauri::async_runtime::block_on(async {
        for failures in [0, 1, 2] {
            let db = fixture();
            let mut replies = vec![];
            for attempt in 0..if failures == 0 { 1 } else { 2 } {
                replies.extend(entity_replies());
                replies.push(Reply::new(404, None, b""));
                replies.push(Reply::new(
                    if attempt < failures { 412 } else { 200 },
                    Some("\"links-new\""),
                    b"",
                ));
            }
            let server = Server::new(replies);
            let result = run(&db, &prepared(&db, &server)).await;
            if failures < 2 {
                let result = result.unwrap();
                assert_eq!(result.link_token.as_deref(), Some("etag:\"links-new\""));
                assert_eq!(result.conflict_retried, failures > 0);
            } else {
                assert_eq!(result.err().unwrap(), "TASK_NOTE_LINK_SYNC_CONFLICT");
            }
            for _ in 0..if failures == 0 { 1 } else { 2 } {
                for (method, key) in [
                    ("GET", "todos.json"),
                    ("GET", "recurrence-rules.json"),
                    ("PUT", "todos.json"),
                    ("GET", "notes.json"),
                    ("PUT", "notes.json"),
                    ("GET", "task-note-links.json"),
                    ("PUT", "task-note-links.json"),
                ] {
                    let r = server.request();
                    assert!(r
                        .head
                        .starts_with(&format!("{method} /rules-test/account/{key} ")));
                    if key == "task-note-links.json" && method == "PUT" {
                        assert!(r.head.to_lowercase().contains("if-none-match: *"));
                        protocol::parse_document(std::str::from_utf8(&r.body).unwrap()).unwrap();
                    }
                }
            }
            let conn = db.connection.lock().unwrap();
            let state = store::snapshot(&conn).unwrap();
            assert_eq!(state.revision == state.synced_revision, failures < 2);
            let dirty = sync_runtime_state::get_snapshot(&conn)
                .unwrap()
                .dirty_domains;
            assert!(!dirty.contains(&"todos".into()));
            assert!(!dirty.contains(&"notes".into()));
            assert_eq!(dirty.contains(&"links".into()), failures == 2);
        }
    });
}

#[test]
fn failures_and_late_reply_preserve_dirty_without_touching_other_entities() {
    tauri::async_runtime::block_on(async {
        for status in [403, 500] {
            let db = fixture();
            let mut replies = entity_replies();
            replies.push(Reply::new(status, None, b"secret body"));
            let server = Server::new(replies);
            let error = run(&db, &prepared(&db, &server)).await.err().unwrap();
            assert_eq!(error, format!("TASK_NOTE_LINK_DOWNLOAD_HTTP:{status}"));
            assert_eq!(
                store::snapshot(&db.connection.lock().unwrap())
                    .unwrap()
                    .synced_revision,
                0
            );
        }
        for config in [false, true] {
            let db = Arc::new(fixture());
            let captured = db.clone();
            let mut replies = entity_replies();
            replies.push(Reply::new(404, None, b""));
            replies.push(Reply::new(200, Some("\"late\""), b"").with_hook(move || {
                let conn = captured.connection.lock().unwrap();
                if config {
                    crate::sync_target::invalidate(&conn).unwrap();
                    crate::sync_target::activate(&conn).unwrap();
                } else {
                    conn.execute("UPDATE notes SET content='edited during upload'", [])
                        .unwrap();
                }
            }));
            let server = Server::new(replies);
            let result = run(&db, &prepared(&db, &server)).await;
            if config {
                assert_eq!(result.err().unwrap(), "RECURRENCE_CONFIG_CHANGED");
            } else {
                assert!(result.unwrap().link_token.is_none());
            }
            assert_eq!(
                store::snapshot(&db.connection.lock().unwrap())
                    .unwrap()
                    .synced_revision,
                0
            );
        }
    });
}

#[test]
fn empty_domain_does_not_create_object_and_transport_rejects_unsafe_responses() {
    tauri::async_runtime::block_on(async {
        let db = fixture();
        db.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM task_note_links", [])
            .unwrap();
        let mut replies = entity_replies();
        replies.push(Reply::new(404, None, b""));
        let server = Server::new(replies);
        assert_eq!(
            run(&db, &prepared(&db, &server))
                .await
                .unwrap()
                .link_token
                .as_deref(),
            Some("missing")
        );
        let state = store::snapshot(&db.connection.lock().unwrap()).unwrap();
        assert_eq!(state.revision, state.synced_revision);
        assert!(state.etag.is_none());
        for (status, etag, body) in [
            (200, None, b"{}".as_slice()),
            (200, Some("W/\"weak\""), b"{}"),
            (200, Some("\"e\""), b"secret invalid"),
            (401, None, b""),
            (403, None, b""),
            (503, None, b""),
        ] {
            let server = Server::new(vec![Reply::new(status, etag, body)]);
            let t =
                TaskNoteLinkTransport::new(&server.bucket(), "account/todos.json", &[]).unwrap();
            assert!(t.download().await.is_err());
        }
        for status in [200, 403, 404, 503] {
            let server = Server::new(vec![Reply::new(status, Some("\"e\""), b"")]);
            let t =
                TaskNoteLinkTransport::new(&server.bucket(), "account/todos.json", &[]).unwrap();
            let result = t.probe().await;
            if status == 503 {
                assert!(result.is_err());
            } else {
                assert_eq!(
                    result.unwrap(),
                    match status {
                        200 => "etag:\"e\"",
                        403 => "denied",
                        _ => "missing",
                    }
                );
            }
        }
        let server = Server::new(vec![
            Reply::new(200, Some("\"old\""), b"{\"format_version\":1,\"links\":[]}"),
            Reply::new(200, Some("\"new\""), b""),
        ]);
        let t = TaskNoteLinkTransport::new(&server.bucket(), "account/todos.json", &[]).unwrap();
        let remote = t.download().await.unwrap();
        let other =
            TaskNoteLinkTransport::new(&server.bucket(), "account/todos.json", &[]).unwrap();
        let doc = protocol::LinkDocument {
            format_version: 1,
            links: vec![],
        };
        assert!(other.upload(&doc, &remote).await.is_err());
        t.upload(&doc, &remote).await.unwrap();
        server.request();
        assert!(server
            .request()
            .head
            .to_lowercase()
            .contains("if-match: \"old\""));
    });
}
