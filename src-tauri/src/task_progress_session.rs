use crate::{
    db::Database, s3_sync::PreparedManualSync, task_checklist_transport::ChecklistUploadOutcome,
    task_progress_protocol as protocol, task_progress_sync as snapshots,
};

pub(crate) struct Receipt {
    snapshot: snapshots::Snapshot,
    token: String,
}
pub(crate) fn final_token(db: &Database, receipt: &Receipt) -> Result<Option<String>, String> {
    let mut connection = db.connection.lock().map_err(|_| "PROGRESS_DATABASE")?;
    Ok(snapshots::is_current(&mut connection, &receipt.snapshot)?.then(|| receipt.token.clone()))
}
pub(crate) async fn attempt(
    db: &Database,
    prepared: &PreparedManualSync,
) -> Result<Option<Receipt>, String> {
    let guard = || -> Result<(), String> {
        let connection = db.connection.lock().map_err(|_| "PROGRESS_DATABASE")?;
        prepared
            .require_current(&connection)
            .map_err(|_| "PROGRESS_CONFIG_CHANGED".into())
    };
    let transport = prepared.progress_transport()?;
    guard()?;
    let remote = transport.download().await;
    guard()?;
    let remote = remote?;
    let snapshot = {
        let mut connection = db.connection.lock().map_err(|_| "PROGRESS_DATABASE")?;
        prepared
            .require_current(&connection)
            .map_err(|_| "PROGRESS_CONFIG_CHANGED")?;
        snapshots::prepare(
            &mut connection,
            prepared.epoch(),
            remote.document.as_deref(),
            remote.etag(),
        )?
    };
    {
        let mut connection = db.connection.lock().map_err(|_| "PROGRESS_DATABASE")?;
        if !snapshots::is_current(&mut connection, &snapshot)? {
            return Ok(None);
        }
    }
    let etag =
        if remote.document.is_none() && protocol::parse(&snapshot.document)?.entries.is_empty() {
            None
        } else if remote.document.as_deref() == Some(snapshot.document.as_str()) {
            guard()?;
            remote.etag().map(str::to_owned)
        } else {
            guard()?;
            let result = transport.upload(&snapshot.document, &remote).await;
            guard()?;
            match result? {
                ChecklistUploadOutcome::Conflict => return Ok(None),
                ChecklistUploadOutcome::Uploaded { etag } => Some(etag),
            }
        };
    let mut connection = db.connection.lock().map_err(|_| "PROGRESS_DATABASE")?;
    if !snapshots::acknowledge(&mut connection, &snapshot, etag.as_deref())? {
        return Ok(None);
    }
    Ok(Some(Receipt {
        snapshot,
        token: etag
            .map(|e| format!("etag:{e}"))
            .unwrap_or("missing".into()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence_transport::tests::{Reply, Server};
    use rusqlite::Connection;
    use std::sync::Mutex;

    #[test]
    fn unchanged_remote_progress_is_acknowledged_with_get_only() {
        tauri::async_runtime::block_on(async {
            let mut connection = Connection::open_in_memory().unwrap();
            crate::db::migrate(&mut connection).unwrap();
            let db = Database {
                connection: Mutex::new(connection),
            };
            let raw = serde_json::json!({"format_version":1,"entries":[{
                "uuid":"123e4567-e89b-42d3-a456-426614174001",
                "task_uuid":"123e4567-e89b-42d3-a456-426614174000",
                "body":"remote","created_at":1,"created_by":"remote",
                "updated_at":1,"updated_by":"remote","clock":1,"deleted_at":null
            }]})
            .to_string();
            let server = Server::new(vec![Reply::new(200, Some("\"same\""), raw.as_bytes())]);
            let prepared = PreparedManualSync::from_test_bucket(
                &db.connection.lock().unwrap(),
                server.bucket(),
            );
            let receipt = attempt(&db, &prepared).await.unwrap().unwrap();
            assert!(server.request().head.starts_with("GET "));
            assert_eq!(
                final_token(&db, &receipt).unwrap().as_deref(),
                Some("etag:\"same\"")
            );
            let state =
                crate::task_progress_store::snapshot(&mut db.connection.lock().unwrap()).unwrap();
            assert_eq!(state.revision, state.synced_revision);
            assert_eq!(state.document.entries.len(), 1);
        });
    }
}
