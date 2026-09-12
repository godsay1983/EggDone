//! Atomic link upload preparation. The transport must upload both captured entity documents first.
use crate::{
    note_sync, sync, sync_target, task_note_link_protocol as protocol,
    task_note_link_store as store,
};
use rusqlite::{params, Connection, TransactionBehavior};

#[derive(Debug, PartialEq, Eq)]
pub struct LinkUploadSnapshot {
    pub todo_json: String,
    pub note_json: String,
    pub links_json: String,
    pub todo_revision: i64,
    pub note_revision: i64,
    pub link_revision: i64,
    epoch: String,
    generated_at: i64,
}

fn db_error(error: rusqlite::Error) -> String {
    format!("TASK_NOTE_LINK_DATABASE: {error}")
}

fn require_epoch(db: &Connection, epoch: &str) -> Result<(), String> {
    if !matches_epoch(db, epoch)? {
        return Err("TASK_NOTE_LINK_CONFIG_CHANGED".into());
    }
    Ok(())
}

fn matches_epoch(db: &Connection, epoch: &str) -> Result<bool, String> {
    Ok(!epoch.is_empty() && !epoch.starts_with("pending:") && sync_target::is_current(db, epoch)?)
}

fn capture(db: &Connection, epoch: &str, now: i64) -> Result<LinkUploadSnapshot, String> {
    let (todo_revision, note_revision): (i64, i64) = db
        .query_row(
            "SELECT todos_dirty_version,notes_dirty_version FROM sync_runtime_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db_error)?;
    let links = store::snapshot(db)?;
    if [todo_revision, note_revision, links.revision]
        .iter()
        .any(|v| !(0..=protocol::MAX_CLOCK).contains(v))
    {
        return Err("TASK_NOTE_LINK_INVALID_SNAPSHOT".into());
    }
    let todos = sync::build_document(db, now)?;
    let notes = note_sync::build_document(db, now)?;
    sync::validate_document(&todos)?;
    note_sync::validate_document(&notes)?;
    Ok(LinkUploadSnapshot {
        todo_json: serde_json::to_string(&todos).map_err(|e| e.to_string())?,
        note_json: serde_json::to_string(&notes).map_err(|e| e.to_string())?,
        links_json: protocol::encode_document(&links.document)?,
        todo_revision,
        note_revision,
        link_revision: links.revision,
        epoch: epoch.into(),
        generated_at: now,
    })
}

/// Call after entity merge/repeat reconciliation. Missing endpoints are not deletion evidence.
pub fn prepare(
    db: &mut Connection,
    epoch: &str,
    incoming: &protocol::LinkDocument,
    now: i64,
) -> Result<LinkUploadSnapshot, String> {
    if !(0..=protocol::MAX_CLOCK).contains(&now) {
        return Err("TASK_NOTE_LINK_INVALID_SNAPSHOT".into());
    }
    protocol::encode_document(incoming)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    require_epoch(&tx, epoch)?;
    store::merge_in_transaction(&tx, incoming)?;
    reconcile_in_transaction(&tx, now)?;
    let result = capture(&tx, epoch, now)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}

pub(crate) fn reconcile_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    now: i64,
) -> Result<(), String> {
    if !(0..=protocol::MAX_CLOCK).contains(&now) {
        return Err("TASK_NOTE_LINK_INVALID_SNAPSHOT".into());
    }
    let by = crate::db::device_id(tx).map_err(db_error)?;
    let mut deleted = store::snapshot(tx)?.document;
    deleted.links.retain(|l| l.deleted_at.is_none());
    let mut reconciled = Vec::new();
    for mut link in deleted.links {
        let entity_clock: Option<i64> = tx.query_row(
            "SELECT MAX(clock) FROM (SELECT MAX(updated_at,deleted_at) AS clock FROM todos WHERE uuid=?1 AND deleted_at IS NOT NULL
             UNION ALL SELECT MAX(updated_at,deleted_at) AS clock FROM notes WHERE uuid=?2 AND deleted_at IS NOT NULL)",
            params![link.todo_uuid, link.note_uuid], |r| r.get(0),
        ).map_err(db_error)?;
        if let Some(clock) = entity_clock {
            let stamp = now.max(clock).max(link.updated_at.saturating_add(1));
            if !(0..=protocol::MAX_CLOCK).contains(&stamp) {
                return Err("TASK_NOTE_LINK_CLOCK_EXHAUSTED".into());
            }
            link.updated_at = stamp;
            link.deleted_at = Some(stamp);
            link.updated_by = by.clone();
            reconciled.push(link);
        }
    }
    store::merge_in_transaction(
        tx,
        &protocol::LinkDocument {
            format_version: 1,
            links: reconciled,
        },
    )?;
    Ok(())
}

/// Recheck before uploading links; versions and exact payloads protect against concurrent entity merges.
pub fn is_current(db: &mut Connection, snapshot: &LinkUploadSnapshot) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    if !matches_epoch(&tx, &snapshot.epoch)? {
        return Ok(false);
    }
    let current = capture(&tx, &snapshot.epoch, snapshot.generated_at)?;
    tx.commit().map_err(db_error)?;
    Ok(current == *snapshot)
}

/// Only call after conditional link PUT succeeds following BOTH entity uploads. No network I/O here.
pub fn acknowledge(
    db: &mut Connection,
    snapshot: &LinkUploadSnapshot,
    etag: &str,
) -> Result<bool, String> {
    if etag.trim().is_empty() || etag.len() > 4096 || etag.chars().any(char::is_control) {
        return Err("TASK_NOTE_LINK_INVALID_ETAG".into());
    }
    acknowledge_value(db, snapshot, Some(etag))
}

/// A successful GET 404 can acknowledge an empty local domain without creating an empty object.
pub(crate) fn acknowledge_missing(
    db: &mut Connection,
    snapshot: &LinkUploadSnapshot,
) -> Result<bool, String> {
    if !protocol::parse_document(&snapshot.links_json)?
        .links
        .is_empty()
    {
        return Err("TASK_NOTE_LINK_NOT_EMPTY".into());
    }
    acknowledge_value(db, snapshot, None)
}

fn acknowledge_value(
    db: &mut Connection,
    snapshot: &LinkUploadSnapshot,
    etag: Option<&str>,
) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    if !matches_epoch(&tx, &snapshot.epoch)? {
        return Ok(false);
    }
    if capture(&tx, &snapshot.epoch, snapshot.generated_at)? != *snapshot {
        return Ok(false);
    }
    tx.execute(
        "UPDATE task_note_link_sync_state SET synced_revision=?1,etag=?2 WHERE id=1",
        params![snapshot.link_revision, etag],
    )
    .map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    Ok(true)
}

#[cfg(test)]
#[path = "task_note_link_sync_tests.rs"]
mod tests;
