//! Exercises the production sync core, not the Tauri window/credential wrapper.
use super::*;
use crate::recurrence_transport::tests::{Reply, Server};
use crate::{task_note_link_protocol as protocol, task_note_link_store as links};
use s3::Bucket;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
const TODO2: &str = "123e4567-e89b-42d3-a456-426614174010";
const NOTE2: &str = "123e4567-e89b-42d3-a456-426614174011";
const ASSET: &str = "123e4567-e89b-42d3-a456-426614174020";
const BYTES: &[u8] = b"# Isolated sync fixture\nDo not use a user bucket.\n";

struct Client {
    db: Database,
    runtime: SyncRuntime,
    assets: NoteAssetStore,
    root: std::path::PathBuf,
    notifications: AtomicUsize,
}

impl Client {
    fn new(todo: &str, note: &str) -> Self {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut connection).unwrap();
        let by = crate::db::device_id(&connection).unwrap();
        connection.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'offline task',0,100,100,?2)", params![todo, by]).unwrap();
        connection.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'offline note','original body','default',0,100,100,?2)", params![note, by]).unwrap();
        links::merge(
            &mut connection,
            &protocol::LinkDocument {
                format_version: 1,
                links: vec![protocol::TaskNoteLink {
                    uuid: protocol::link_uuid(todo, note).unwrap(),
                    todo_uuid: todo.into(),
                    note_uuid: note.into(),
                    created_at: 100,
                    updated_at: 100,
                    updated_by: by,
                    deleted_at: None,
                }],
            },
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!("eggdone-sync-core-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self {
            db: Database {
                connection: Mutex::new(connection),
            },
            runtime: SyncRuntime::default(),
            assets: NoteAssetStore::for_root(root.clone()),
            root,
            notifications: AtomicUsize::new(0),
        }
    }

    async fn sync(&self, bucket: Box<Bucket>) -> Result<ManualSyncResult, String> {
        // The command holds this same guard. Credential lookup and UI events stay out of this test.
        let _guard = self.runtime.acquire()?;
        let prepared = s3_sync::PreparedManualSync::from_test_bucket(
            &self.db.connection.lock().unwrap(),
            bucket,
        );
        sync_now_inner(&self.db, &self.runtime, &self.assets, &prepared, || {
            self.notifications.fetch_add(1, Ordering::SeqCst);
        })
        .await
    }

    fn state(&self) -> sync_runtime_state::SyncRuntimeSnapshot {
        sync_runtime_state::get_snapshot(&self.db.connection.lock().unwrap()).unwrap()
    }

    fn add_file(&self) {
        let file = self
            .assets
            .import_file_bytes(BYTES, "fixture.md", ASSET)
            .unwrap();
        note_attachments::create_pending(
            &self.db.connection.lock().unwrap(),
            &note_attachments::NewNoteAttachment {
                uuid: ASSET.into(),
                note_uuid: NOTE.into(),
                kind: "file".into(),
                display_name: file.display_name,
                mime_type: file.mime_type,
                byte_size: file.byte_size,
                sha256: file.sha256,
                preview_mime_type: None,
                preview_byte_size: None,
                preview_sha256: None,
                width: None,
                height: None,
                sort_order: 0,
                local_original_path: file.local_original_path,
                local_preview_path: None,
            },
        )
        .unwrap();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(root.parent(), Some(temp.as_path()));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("eggdone-sync-core-"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn entities(link_status: u16) -> Vec<Reply> {
    vec![
        Reply::new(404, None, b""),
        Reply::new(404, None, b""),
        Reply::new(200, None, b""),
        Reply::new(404, None, b""),
        Reply::new(200, None, b""),
        Reply::new(404, None, b""),
        Reply::new(link_status, Some("\"link\""), b""),
    ]
}

fn tail() -> Vec<Reply> {
    vec![
        Reply::new(404, None, b""),
        Reply::new(200, None, b""),
        Reply::new(200, Some("\"todos\""), b""),
        Reply::new(200, Some("\"notes\""), b""),
        Reply::new(200, Some("\"attachments\""), b""),
        Reply::new(404, None, b""),
        Reply::new(200, Some("\"link\""), b""),
    ]
}

fn assert_entity_order(server: &Server) {
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
        assert!(
            r.head
                .starts_with(&format!("{method} /rules-test/account/{key} ")),
            "{}",
            r.head
        );
        if method == "PUT" {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            assert!(body.is_object());
        }
    }
}

#[test]
fn core_orders_entities_links_attachments_and_final_probes() {
    tauri::async_runtime::block_on(async {
        let client = Client::new(TODO, NOTE);
        let mut replies = entities(200);
        replies.extend(tail());
        let server = Server::new(replies);
        let result = client.sync(server.bucket()).await.unwrap();
        assert_eq!(
            (
                result.todo_count,
                result.note_count,
                result.note_attachment_count
            ),
            (1, 1, 0)
        );
        assert_eq!(result.link_remote_token.as_deref(), Some("etag:\"link\""));
        assert_eq!(result.todo_remote_etag.as_deref(), Some("\"todos\""));
        assert_eq!(result.note_remote_etag.as_deref(), Some("\"notes\""));
        assert_eq!(
            result.note_attachment_remote_etag.as_deref(),
            Some("\"attachments\"")
        );
        assert!(client.state().dirty_domains.is_empty());
        assert_eq!(client.notifications.load(Ordering::SeqCst), 1);
        assert_entity_order(&server);
        for (method, key) in [
            ("GET", "note-attachments.json"),
            ("PUT", "note-attachments.json"),
            ("HEAD", "todos.json"),
            ("HEAD", "notes.json"),
            ("HEAD", "note-attachments.json"),
            ("HEAD", "recurrence-rules.json"),
            ("HEAD", "task-note-links.json"),
        ] {
            assert!(server
                .request()
                .head
                .starts_with(&format!("{method} /rules-test/account/{key} ")));
        }
    });
}

#[test]
fn core_link_conflicts_are_bounded_and_reupload_entities_before_retry() {
    tauri::async_runtime::block_on(async {
        for conflicts in [1, 2] {
            let client = Client::new(TODO, NOTE);
            let mut replies = entities(412);
            replies.extend(entities(if conflicts == 1 { 200 } else { 412 }));
            if conflicts == 1 {
                replies.extend(tail());
            }
            let server = Server::new(replies);
            let result = client.sync(server.bucket()).await;
            if conflicts == 1 {
                assert!(result.unwrap().conflict_retried);
                assert!(client.state().dirty_domains.is_empty());
            } else {
                assert_eq!(result.err().unwrap(), "TASK_NOTE_LINK_SYNC_CONFLICT");
                assert_eq!(client.state().dirty_domains, vec!["links"]);
                assert_eq!(client.notifications.load(Ordering::SeqCst), 0);
            }
            assert_entity_order(&server);
            assert_entity_order(&server);
            assert!(client.runtime.acquire().is_ok());
        }
    });
}

#[test]
fn core_partial_failure_preserves_link_ack_and_recovers_attachment_metadata() {
    tauri::async_runtime::block_on(async {
        let client = Client::new(TODO, NOTE);
        client.add_file();
        let mut replies = entities(200);
        replies.push(Reply::new(200, None, b"")); // Immutable binary succeeds before metadata fails.
        replies.push(Reply::new(403, None, b""));
        let server = Server::new(replies);
        assert!(client
            .sync(server.bucket())
            .await
            .err()
            .unwrap()
            .contains("附件元数据"));
        assert_eq!(client.state().dirty_domains, vec!["attachments"]);
        assert_entity_order(&server);
        assert!(server
            .request()
            .head
            .contains(&format!("PUT /rules-test/account/assets/{ASSET}/original ")));
        assert!(server
            .request()
            .head
            .contains("GET /rules-test/account/note-attachments.json "));
        let attachment =
            note_attachments::list_active_by_note(&client.db.connection.lock().unwrap(), NOTE)
                .unwrap()
                .remove(0);
        assert!(attachment.remote_uploaded);
        assert_eq!(attachment.transfer_state, "uploaded");
        assert_eq!(client.notifications.load(Ordering::SeqCst), 0);

        let mut replies = entities(200);
        replies.push(Reply::new(200, None, b""));
        replies.extend(tail());
        let recovery = Server::new(replies);
        let result = client.sync(recovery.bucket()).await.unwrap();
        assert_eq!(result.pending_attachment_count, 0);
        assert_eq!(result.note_attachment_count, 1);
        assert!(client.state().dirty_domains.is_empty());
        assert!(client.runtime.acquire().is_ok());
    });
}

#[test]
fn core_metadata_conflict_budget_and_late_edit_do_not_lose_pending_changes() {
    tauri::async_runtime::block_on(async {
        let client = Arc::new(Client::new(TODO, NOTE));
        let mut replies = entities(200);
        for _ in 0..2 {
            replies.extend([Reply::new(404, None, b""), Reply::new(412, None, b"")]);
        }
        let server = Server::new(replies);
        assert!(client
            .sync(server.bucket())
            .await
            .err()
            .unwrap()
            .contains("附件元数据持续发生变化"));
        let snapshot = links::snapshot(&client.db.connection.lock().unwrap()).unwrap();
        assert_eq!(snapshot.revision, snapshot.synced_revision);
        assert_eq!(client.notifications.load(Ordering::SeqCst), 0);
        assert!(client.runtime.acquire().is_ok());

        let captured = client.clone();
        let mut replies = entities(200);
        let mut final_replies = tail();
        final_replies[1] = Reply::new(200, None, b"").with_hook(move || {
            assert!(captured.runtime.acquire().is_err());
            captured
                .db
                .connection
                .lock()
                .unwrap()
                .execute("UPDATE notes SET content='late edit',updated_at=200", [])
                .unwrap();
        });
        replies.extend(final_replies);
        let late = Server::new(replies);
        client.sync(late.bucket()).await.unwrap();
        assert!(client.state().dirty_domains.contains(&"notes".into()));
        assert!(client.runtime.acquire().is_ok());
    });
}

#[test]
fn core_target_change_during_attachment_reply_does_not_ack_the_new_target() {
    tauri::async_runtime::block_on(async {
        let client = Arc::new(Client::new(TODO, NOTE));
        let captured = client.clone();
        let mut replies = entities(200);
        replies.push(Reply::new(404, None, b""));
        replies.push(Reply::new(200, None, b"").with_hook(move || {
            let db = captured.db.connection.lock().unwrap();
            crate::sync_target::invalidate(&db).unwrap();
            crate::sync_target::activate(&db).unwrap();
        }));
        let server = Server::new(replies);
        assert_eq!(
            client.sync(server.bucket()).await.err().unwrap(),
            "RECURRENCE_CONFIG_CHANGED"
        );
        assert_eq!(client.notifications.load(Ordering::SeqCst), 0);
        let snapshot = links::snapshot(&client.db.connection.lock().unwrap()).unwrap();
        assert_eq!(snapshot.revision, snapshot.synced_revision + 1);
        assert!(snapshot.etag.is_none());
        for domain in ["todos", "notes", "attachments", "links"] {
            assert!(client.state().dirty_domains.contains(&domain.into()));
        }
        assert!(client.runtime.acquire().is_ok());
    });
}

#[path = "sync_core_s3_tests.rs"]
mod s3_tests;
