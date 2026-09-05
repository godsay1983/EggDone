//! Production I/O adapter. Not yet routed from the public sync command: discovery and UI refresh
//! must be integrated together before enabling custom rules.
use crate::{
    db::Database,
    recurrence_protocol,
    recurrence_snapshot::{self, RecurrenceUploadSnapshot, SnapshotPreparation},
    recurrence_store,
    recurrence_sync_flow::{self, RecurrenceSyncPort, RecurrenceSyncResult},
    recurrence_transport::{RecurrenceTransport, RemoteRules, RuleUploadOutcome},
    s3_sync::{self, PreparedManualSync, RemoteSyncObject, SyncRuntime},
    sync,
    sync_runtime_state::{self, SyncDomain},
};
use rusqlite::Connection;

struct RemotePair {
    todos: RemoteSyncObject,
    rules: RemoteRules,
}

struct Session<'a> {
    database: &'a Database,
    prepared: PreparedManualSync,
    rules: RecurrenceTransport,
}

// The caller must perform attempt/error reporting and reminder/UI refresh even on partial failure.
pub async fn sync_pair(
    database: &Database,
    runtime: &SyncRuntime,
) -> Result<RecurrenceSyncResult, String> {
    let _guard = runtime.acquire()?;
    let prepared = {
        let db = database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        s3_sync::prepare_manual_sync(&db)?
    };
    let rules = prepared.recurrence_transport()?;
    recurrence_sync_flow::run(&mut Session {
        database,
        prepared,
        rules,
    })
    .await
}

impl Session<'_> {
    fn require_current(&self, db: &Connection) -> Result<(), String> {
        if self.prepared.target_is_current(db)? {
            Ok(())
        } else {
            Err("RECURRENCE_CONFIG_CHANGED".into())
        }
    }
    fn check(&self) -> Result<(), String> {
        let db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        self.require_current(&db)
    }
}

impl RecurrenceSyncPort for Session<'_> {
    type Remote = RemotePair;
    async fn target_is_current(&mut self) -> Result<bool, String> {
        let db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        self.prepared.target_is_current(&db)
    }
    async fn download(&mut self) -> Result<RemotePair, String> {
        self.check()?;
        let todos = s3_sync::download_remote(&self.prepared).await?;
        self.check()?;
        let rules = self.rules.download().await?;
        self.check()?;
        Ok(RemotePair { todos, rules })
    }
    async fn prepare(&mut self, remote: &RemotePair) -> Result<SnapshotPreparation, String> {
        let mut db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        self.require_current(&db)?;
        if let Some(document) = &remote.todos.document {
            sync::merge_remote_document(&mut db, document, crate::db::now_millis())?;
        }
        if let Some(document) = &remote.rules.document {
            recurrence_store::merge(&mut db, document)?;
        }
        recurrence_snapshot::prepare_snapshot(&mut db, crate::db::now_millis())
    }
    async fn upload_todos(
        &mut self,
        remote: &RemotePair,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<bool, String> {
        let document: sync::SyncDocument =
            serde_json::from_str(&snapshot.todo_json).map_err(|_| "INVALID_RECURRENCE_SNAPSHOT")?;
        sync::validate_document(&document)?;
        self.check()?;
        let result = s3_sync::upload_document(&self.prepared, &document, &remote.todos).await?;
        self.check()?;
        Ok(matches!(result, s3_sync::UploadOutcome::Success))
    }
    async fn acknowledge_todos(&mut self, revision: i64) -> Result<bool, String> {
        let db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        let tx = db
            .unchecked_transaction()
            .map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        self.require_current(&tx)?;
        let result = sync_runtime_state::mark_domain_synced(&tx, SyncDomain::Todos, revision)?;
        tx.commit().map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        Ok(result)
    }
    async fn upload_rules(
        &mut self,
        remote: &RemotePair,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<RuleUploadOutcome, String> {
        let document = recurrence_protocol::parse_document(&snapshot.rules_json)?;
        self.check()?;
        let result = self.rules.upload(&document, &remote.rules).await?;
        self.check()?;
        Ok(result)
    }
    async fn acknowledge_rules(&mut self, revision: i64, etag: &str) -> Result<bool, String> {
        let db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        let tx = db
            .unchecked_transaction()
            .map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        self.require_current(&tx)?;
        let result = recurrence_store::acknowledge(&tx, revision, etag)?;
        tx.commit().map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        Ok(result)
    }
    async fn snapshot_is_current(
        &mut self,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<bool, String> {
        let db = self
            .database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        let tx = db
            .unchecked_transaction()
            .map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        self.require_current(&tx)?;
        let todo: i64 = tx
            .query_row(
                "SELECT todos_dirty_version FROM sync_runtime_state WHERE id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        let rules = recurrence_store::snapshot(&tx)?;
        tx.commit().map_err(|_| "RECURRENCE_DATABASE_TRANSACTION")?;
        Ok(todo == snapshot.todo_revision && rules.revision == snapshot.rule_revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence_transport::tests::{Reply, Server};
    use std::sync::Mutex;

    fn session<'a>(database: &'a Database, server: &Server) -> Session<'a> {
        let prepared = PreparedManualSync::from_test_bucket(
            &database.connection.lock().unwrap(),
            server.bucket(),
        );
        let rules = prepared.recurrence_transport().unwrap();
        Session {
            database,
            prepared,
            rules,
        }
    }

    #[test]
    fn real_s3_adapter_replays_whole_pair_after_rule_conflict() {
        tauri::async_runtime::block_on(async {
            let server = Server::new(vec![
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(200, None, b""),
                Reply::new(412, None, b""),
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(200, None, b""),
                Reply::new(200, Some("\"rules-new\""), b""),
            ]);
            let database = Database {
                connection: Mutex::new(recurrence_snapshot::tests::setup(true, false, false)),
            };
            let result = recurrence_sync_flow::run(&mut session(&database, &server))
                .await
                .unwrap();
            assert_eq!(result.attempts, 2);
            assert!(!result.local_changes_pending);
            let mut first_todo = String::new();
            for i in 0..8 {
                let request = server.request();
                let method = if i % 4 < 2 { "GET" } else { "PUT" };
                let key = if i % 2 == 0 {
                    "todos.json"
                } else {
                    "recurrence-rules.json"
                };
                assert!(request
                    .head
                    .starts_with(&format!("{method} /rules-test/account/{key} ")));
                if method == "PUT" {
                    assert!(request.head.to_lowercase().contains("if-none-match: *"));
                    let body = String::from_utf8(request.body).unwrap();
                    if i == 2 {
                        first_todo = body;
                    } else if i == 6 {
                        let first: sync::SyncDocument = serde_json::from_str(&first_todo).unwrap();
                        let second: sync::SyncDocument = serde_json::from_str(&body).unwrap();
                        assert_eq!(first.todos.len(), second.todos.len());
                        assert_eq!(first.todos.len(), 2);
                    }
                }
            }
            let db = database.connection.lock().unwrap();
            let rules = recurrence_store::snapshot(&db).unwrap();
            assert_eq!(rules.revision, rules.synced_revision);
            assert_eq!(rules.etag.as_deref(), Some("\"rules-new\""));
        });
    }

    #[test]
    fn config_change_rejects_prepare_and_both_late_acknowledgements() {
        tauri::async_runtime::block_on(async {
            let server = Server::new(vec![Reply::new(404, None, b""), Reply::new(404, None, b"")]);
            let database = Database {
                connection: Mutex::new(recurrence_snapshot::tests::setup(false, false, false)),
            };
            let mut port = session(&database, &server);
            let remote = port.download().await.unwrap();
            let snapshot = port.prepare(&remote).await.unwrap().snapshot.unwrap();
            {
                let db = database.connection.lock().unwrap();
                crate::sync_target::invalidate(&db).unwrap();
                crate::sync_target::activate(&db).unwrap();
            }
            assert_eq!(
                port.prepare(&remote).await.err().unwrap(),
                "RECURRENCE_CONFIG_CHANGED"
            );
            assert_eq!(
                port.acknowledge_todos(snapshot.todo_revision)
                    .await
                    .unwrap_err(),
                "RECURRENCE_CONFIG_CHANGED"
            );
            assert_eq!(
                port.acknowledge_rules(snapshot.rule_revision, "\"late\"")
                    .await
                    .unwrap_err(),
                "RECURRENCE_CONFIG_CHANGED"
            );
            assert_eq!(
                port.snapshot_is_current(&snapshot).await.unwrap_err(),
                "RECURRENCE_CONFIG_CHANGED"
            );
            assert_eq!(
                port.upload_rules(&remote, &snapshot).await.unwrap_err(),
                "RECURRENCE_CONFIG_CHANGED"
            );
            let db = database.connection.lock().unwrap();
            let rules = recurrence_store::snapshot(&db).unwrap();
            assert!(rules.revision > rules.synced_revision);
            assert!(rules.etag.is_none());
        });
    }

    #[test]
    fn real_adapter_keeps_edits_after_snapshot_pending() {
        tauri::async_runtime::block_on(async {
            let server = Server::new(vec![
                Reply::new(404, None, b""),
                Reply::new(404, None, b""),
                Reply::new(200, None, b""),
            ]);
            let database = Database {
                connection: Mutex::new(recurrence_snapshot::tests::setup(false, false, false)),
            };
            let mut port = session(&database, &server);
            let remote = port.download().await.unwrap();
            let snapshot = port.prepare(&remote).await.unwrap().snapshot.unwrap();
            assert!(port.upload_todos(&remote, &snapshot).await.unwrap());
            database
                .connection
                .lock()
                .unwrap()
                .execute(
                    "UPDATE todos SET title='new local edit',updated_at=updated_at+1",
                    [],
                )
                .unwrap();
            assert!(!port
                .acknowledge_todos(snapshot.todo_revision)
                .await
                .unwrap());
            assert!(!port.snapshot_is_current(&snapshot).await.unwrap());
        });
    }
}
