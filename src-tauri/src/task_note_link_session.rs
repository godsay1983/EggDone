//! Production Todo+note+link coordination, under the caller's whole-session SyncRuntime guard.
use crate::{
    db::{now_millis, Database},
    note_sync,
    recurrence_sync_session::{self, TodoRunResult},
    s3_sync::{self, PreparedManualSync, UploadOutcome},
    sync,
    sync_runtime_state::{self, SyncDomain},
    task_note_link_protocol as protocol, task_note_link_sync as snapshots,
    task_note_link_transport::LinkUploadOutcome,
};

pub(crate) struct EntityLinkResult {
    pub todo: TodoRunResult,
    pub note_count: usize,
    pub conflict_retried: bool,
    pub link_token: Option<String>,
}

fn guard(db: &Database, prepared: &PreparedManualSync) -> Result<(), String> {
    let db = db
        .connection
        .lock()
        .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
    prepared.require_current(&db)
}

async fn sync_notes(
    db: &Database,
    prepared: &PreparedManualSync,
) -> Result<(note_sync::NoteSyncDocument, bool), String> {
    guard(db, prepared)?;
    let mut remote = s3_sync::download_note_remote(prepared).await?;
    guard(db, prepared)?;
    for attempt in 0..2 {
        let (document, revision) = {
            let mut db = db
                .connection
                .lock()
                .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
            prepared.require_current(&db)?;
            let document = match &remote.document {
                Some(doc) => note_sync::merge_remote_document(&mut db, doc, now_millis())?,
                None => note_sync::build_document(&db, now_millis())?,
            };
            (
                document,
                sync_runtime_state::domain_revision(&db, SyncDomain::Notes)?,
            )
        };
        guard(db, prepared)?;
        let result = s3_sync::upload_note_document(prepared, &document, &remote).await?;
        guard(db, prepared)?;
        if result == UploadOutcome::Success {
            let db = db
                .connection
                .lock()
                .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
            prepared.require_current(&db)?;
            sync_runtime_state::mark_domain_synced(&db, SyncDomain::Notes, revision)?;
            return Ok((document, attempt > 0));
        }
        if attempt == 0 {
            remote = s3_sync::download_note_remote(prepared).await?;
            guard(db, prepared)?;
        }
    }
    Err("TASK_NOTE_LINK_SYNC_CONFLICT".into())
}

fn receipts_match(
    snapshot: &snapshots::LinkUploadSnapshot,
    todo: &sync::SyncDocument,
    note: &note_sync::NoteSyncDocument,
) -> Result<bool, String> {
    let current_todo: sync::SyncDocument =
        serde_json::from_str(&snapshot.todo_json).map_err(|_| "TASK_NOTE_LINK_INVALID_SNAPSHOT")?;
    let current_note: note_sync::NoteSyncDocument =
        serde_json::from_str(&snapshot.note_json).map_err(|_| "TASK_NOTE_LINK_INVALID_SNAPSHOT")?;
    Ok(current_todo.todos == todo.todos
        && current_todo.groups == todo.groups
        && current_note.notes == note.notes)
}

pub(crate) async fn run(
    db: &Database,
    prepared: &PreparedManualSync,
) -> Result<EntityLinkResult, String> {
    let transport = prepared.link_transport()?;
    let mut conflict_retried = false;
    for attempt in 0..2 {
        let todo = recurrence_sync_session::sync_todos(db, prepared.clone()).await?;
        let (notes, note_retry) = sync_notes(db, prepared)
            .await
            .map_err(|e| format!("便签同步失败：{e}"))?;
        conflict_retried |= todo.conflict_retried || note_retry || attempt > 0;
        guard(db, prepared)?;
        let remote = transport.download().await;
        guard(db, prepared)?;
        let remote = remote?;
        let snapshot = {
            let mut db = db
                .connection
                .lock()
                .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
            snapshots::prepare(
                &mut db,
                prepared.epoch(),
                remote.document.as_ref().unwrap_or(&protocol::LinkDocument {
                    format_version: 1,
                    links: vec![],
                }),
                now_millis(),
            )?
        };
        if !receipts_match(&snapshot, &todo.uploaded, &notes)? {
            continue;
        }
        let document = protocol::parse_document(&snapshot.links_json)?;
        let absent = remote.document.is_none() && document.links.is_empty();
        {
            let mut db = db
                .connection
                .lock()
                .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
            prepared.require_current(&db)?;
            if !snapshots::is_current(&mut db, &snapshot)? {
                continue;
            }
        }
        let token = if absent {
            None
        } else {
            guard(db, prepared)?;
            let uploaded = transport.upload(&document, &remote).await;
            guard(db, prepared)?;
            match uploaded? {
                LinkUploadOutcome::Conflict => continue,
                LinkUploadOutcome::Uploaded { etag } => Some(etag),
            }
        };
        let ack = {
            let mut db = db
                .connection
                .lock()
                .map_err(|_| "TASK_NOTE_LINK_DATABASE_LOCK")?;
            prepared.require_current(&db)?;
            match &token {
                Some(etag) => snapshots::acknowledge(&mut db, &snapshot, etag)?,
                None => snapshots::acknowledge_missing(&mut db, &snapshot)?,
            }
        };
        return Ok(EntityLinkResult {
            todo,
            note_count: notes.notes.len(),
            conflict_retried,
            link_token: if ack {
                Some(
                    token
                        .map(|v| format!("etag:{v}"))
                        .unwrap_or("missing".into()),
                )
            } else {
                None
            },
        });
    }
    Err("TASK_NOTE_LINK_SYNC_CONFLICT".into())
}

#[cfg(test)]
#[path = "task_note_link_session_tests.rs"]
mod tests;
