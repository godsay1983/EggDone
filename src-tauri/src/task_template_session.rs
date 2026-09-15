use crate::{
    db::Database, s3_sync::PreparedManualSync, task_checklist_transport::ChecklistUploadOutcome,
    task_template_protocol as protocol, task_template_sync as snapshots,
};

pub(crate) async fn attempt(
    db: &Database,
    prepared: &PreparedManualSync,
) -> Result<Option<String>, String> {
    let guard = || -> Result<(), String> {
        let db = db.connection.lock().map_err(|_| "TEMPLATE_DATABASE_LOCK")?;
        prepared.require_current(&db)
    };
    let transport = prepared.template_transport()?;
    guard()?;
    let remote = transport.download().await;
    guard()?;
    let remote = remote?;
    let snapshot = {
        let mut db = db.connection.lock().map_err(|_| "TEMPLATE_DATABASE_LOCK")?;
        snapshots::prepare(
            &mut db,
            prepared.epoch(),
            remote
                .document
                .as_deref()
                .unwrap_or("{\"format_version\":1,\"templates\":[]}"),
        )?
    };
    {
        let mut db = db.connection.lock().map_err(|_| "TEMPLATE_DATABASE_LOCK")?;
        if !snapshots::is_current(&mut db, &snapshot)? {
            return Ok(None);
        }
    }
    let etag =
        if remote.document.is_none() && protocol::parse(&snapshot.document)?.templates.is_empty() {
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
    let mut db = db.connection.lock().map_err(|_| "TEMPLATE_DATABASE_LOCK")?;
    if !snapshots::acknowledge(&mut db, &snapshot, etag.as_deref())? {
        return Ok(None);
    }
    Ok(Some(
        etag.map(|v| format!("etag:{v}"))
            .unwrap_or("missing".into()),
    ))
}
