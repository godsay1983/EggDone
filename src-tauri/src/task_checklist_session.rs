use crate::{
    db::{now_millis, Database},
    s3_sync::PreparedManualSync,
    sync,
    task_checklist_sync::{self as snapshots, Domain},
    task_checklist_transport::ChecklistUploadOutcome,
};

// One attempt under the existing whole-session lock. None asks the owner to repeat parent synchronization.
pub(crate) async fn attempt(
    db: &Database,
    prepared: &PreparedManualSync,
    todo: &sync::SyncDocument,
) -> Result<Option<String>, String> {
    let guard = || -> Result<(), String> {
        let c = db
            .connection
            .lock()
            .map_err(|_| "CHECKLIST_DATABASE_LOCK")?;
        prepared.require_current(&c)
    };
    let definitions = prepared.checklist_transport(Domain::Definitions)?;
    let items = prepared.checklist_transport(Domain::Items)?;
    guard()?;
    let remote_definitions = definitions.download().await?;
    guard()?;
    let remote_items = items.download().await?;
    guard()?;
    let snapshot = {
        let mut c = db
            .connection
            .lock()
            .map_err(|_| "CHECKLIST_DATABASE_LOCK")?;
        prepared.require_current(&c)?;
        snapshots::prepare(
            &mut c,
            prepared.epoch(),
            remote_items
                .document
                .as_deref()
                .unwrap_or(&Domain::Items.empty()),
            remote_definitions
                .document
                .as_deref()
                .unwrap_or(&Domain::Definitions.empty()),
            now_millis(),
        )?
    };
    let captured: sync::SyncDocument =
        serde_json::from_str(&snapshot.todo_json).map_err(|_| "CHECKLIST_SNAPSHOT_INVALID")?;
    if !snapshot.rules_ready || captured.todos != todo.todos || captured.groups != todo.groups {
        return Ok(None);
    }
    let mut tokens = Vec::new();
    for (domain, transport, remote, document) in [
        (
            Domain::Definitions,
            &definitions,
            &remote_definitions,
            &snapshot.definitions,
        ),
        (Domain::Items, &items, &remote_items, &snapshot.items),
    ] {
        guard()?;
        {
            let mut c = db
                .connection
                .lock()
                .map_err(|_| "CHECKLIST_DATABASE_LOCK")?;
            if !snapshots::is_current(&mut c, &snapshot)? {
                return Ok(None);
            }
        }
        let etag = if remote.document.is_none() && domain.is_empty(document)? {
            None
        } else {
            let uploaded = transport.upload(document, remote).await?;
            guard()?;
            match uploaded {
                ChecklistUploadOutcome::Conflict => return Ok(None),
                ChecklistUploadOutcome::Uploaded { etag } => Some(etag),
            }
        };
        let acknowledged = {
            let mut c = db
                .connection
                .lock()
                .map_err(|_| "CHECKLIST_DATABASE_LOCK")?;
            prepared.require_current(&c)?;
            snapshots::acknowledge(&mut c, &snapshot, domain, etag.as_deref())?
        };
        if !acknowledged {
            return Ok(None);
        }
        tokens.push(
            etag.map(|s| format!("etag:{s}"))
                .unwrap_or("missing".into()),
        );
    }
    Ok(Some(
        serde_json::to_string(&tokens).map_err(|_| "CHECKLIST_TOKEN_INVALID")?,
    ))
}
