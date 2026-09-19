use crate::{
    db::Database, s3_sync::PreparedManualSync, task_checklist_transport::ChecklistUploadOutcome,
    task_workflow_protocol as protocol, task_workflow_sync as snapshots,
};

pub(crate) struct Receipt {
    snapshot: snapshots::Snapshot,
    token: String,
}

pub(crate) fn final_token(db: &Database, receipt: &Receipt) -> Result<Option<String>, String> {
    let mut connection = db.connection.lock().map_err(|_| "WORKFLOW_DATABASE_LOCK")?;
    Ok(snapshots::is_current(&mut connection, &receipt.snapshot)?.then(|| receipt.token.clone()))
}

pub(crate) async fn attempt(
    db: &Database,
    prepared: &PreparedManualSync,
) -> Result<Option<Receipt>, String> {
    let guard = || -> Result<(), String> {
        let connection = db.connection.lock().map_err(|_| "WORKFLOW_DATABASE_LOCK")?;
        prepared.require_current(&connection)
    };
    let transport = prepared.workflow_transport()?;
    guard()?;
    let remote = transport.download().await;
    guard()?;
    let remote = remote?;
    let snapshot = {
        let mut connection = db.connection.lock().map_err(|_| "WORKFLOW_DATABASE_LOCK")?;
        prepared.require_current(&connection)?;
        if remote.document.is_none() {
            let prior_etag: Option<String> = connection
                .query_row(
                    "SELECT etag FROM task_workflow_sync_state WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| "WORKFLOW_SYNC_DATABASE")?;
            if prior_etag.is_some() {
                return Err("WORKFLOW_REMOTE_MISSING".into());
            }
        }
        snapshots::prepare(
            &mut connection,
            prepared.epoch(),
            remote.document.as_deref(),
            remote.etag(),
        )?
    };
    {
        let mut connection = db.connection.lock().map_err(|_| "WORKFLOW_DATABASE_LOCK")?;
        if !snapshots::is_current(&mut connection, &snapshot)? {
            return Ok(None);
        }
    }
    let etag =
        if remote.document.is_none() && protocol::is_empty(&protocol::parse(&snapshot.document)?) {
            None
        } else {
            guard()?;
            let result = transport.upload(&snapshot.document, &remote).await;
            guard()?;
            match result? {
                ChecklistUploadOutcome::Conflict => return Ok(None),
                ChecklistUploadOutcome::Uploaded { etag } => Some(etag),
            }
        };
    let mut connection = db.connection.lock().map_err(|_| "WORKFLOW_DATABASE_LOCK")?;
    if !snapshots::acknowledge(&mut connection, &snapshot, etag.as_deref())? {
        return Ok(None);
    }
    Ok(Some(Receipt {
        snapshot,
        token: etag
            .map(|value| format!("etag:{value}"))
            .unwrap_or("missing".into()),
    }))
}
