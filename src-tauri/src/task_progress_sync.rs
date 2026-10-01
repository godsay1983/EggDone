//! Exact document/revision receipts, isolated by target epoch and generation.
use crate::{sync_target, task_progress_protocol as protocol, task_progress_store as store};
use rusqlite::{params, Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

pub use protocol::object_key;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub document: String,
    pub revision: i64,
    pub generation: i64,
    epoch: String,
    digest: String,
}
pub(crate) fn remote_seen(db: &Connection, epoch: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
        [format!("task.progress.remote.seen.v1:{epoch}")],
        |r| r.get(0),
    )
    .map_err(store::error)
}
fn record_seen(db: &Connection, epoch: &str) -> Result<(), String> {
    db.execute(
        "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,'1')",
        [format!("task.progress.remote.seen.v1:{epoch}")],
    )
    .map_err(store::error)?;
    Ok(())
}
fn current(db: &Connection, epoch: &str) -> Result<bool, String> {
    Ok(!epoch.is_empty()
        && !epoch.starts_with("pending:")
        && sync_target::is_current(db, epoch).map_err(|_| "PROGRESS_DATABASE")?)
}
fn capture(db: &Connection, epoch: &str) -> Result<Snapshot, String> {
    let s = store::read_in_transaction(db)?;
    let document = protocol::encode(&s.document)?;
    let digest = format!("{:x}", Sha256::digest(document.as_bytes()));
    Ok(Snapshot {
        document,
        revision: s.revision,
        generation: s.generation,
        epoch: epoch.into(),
        digest,
    })
}
pub fn prepare(
    db: &mut Connection,
    epoch: &str,
    remote: Option<&str>,
    etag: Option<&str>,
) -> Result<Snapshot, String> {
    if remote.is_some() != etag.is_some()
        || etag.is_some_and(|e| !crate::task_checklist_transport::valid_etag(e))
    {
        return Err("PROGRESS_ETAG_REQUIRED".into());
    }
    let document = remote.map(protocol::parse).transpose()?.unwrap_or_default();
    if !current(db, epoch)? {
        return Err("PROGRESS_CONFIG_CHANGED".into());
    }
    // Existence evidence survives a valid GET followed by a failed merge/upload.
    if remote.is_some() {
        record_seen(db, epoch)?;
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(store::error)?;
    if !current(&tx, epoch)? {
        return Err("PROGRESS_CONFIG_CHANGED".into());
    }
    if remote.is_none()
        && (store::read_in_transaction(&tx)?.etag.is_some() || remote_seen(&tx, epoch)?)
    {
        return Err("PROGRESS_REMOTE_MISSING".into());
    }
    if let Some(etag) = etag {
        tx.execute(
            "UPDATE task_progress_sync_state SET etag=?1 WHERE id=1",
            [etag],
        )
        .map_err(store::error)?;
    }
    store::merge_in_transaction(&tx, &document)?;
    let snapshot = capture(&tx, epoch)?;
    tx.commit().map_err(store::error)?;
    Ok(snapshot)
}
pub fn is_current(db: &mut Connection, snapshot: &Snapshot) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(store::error)?;
    Ok(current(&tx, &snapshot.epoch)? && capture(&tx, &snapshot.epoch)? == *snapshot)
}
pub fn acknowledge(
    db: &mut Connection,
    snapshot: &Snapshot,
    etag: Option<&str>,
) -> Result<bool, String> {
    if format!("{:x}", Sha256::digest(snapshot.document.as_bytes())) != snapshot.digest {
        return Ok(false);
    }
    if let Some(etag) = etag {
        if !crate::task_checklist_transport::valid_etag(etag) {
            return Err("PROGRESS_ETAG_REQUIRED".into());
        }
    } else if !protocol::parse(&snapshot.document)?.entries.is_empty() {
        return Err("PROGRESS_NOT_EMPTY".into());
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(store::error)?;
    if !current(&tx, &snapshot.epoch)? {
        return Ok(false);
    }
    let now = capture(&tx, &snapshot.epoch)?;
    let synced_revision: i64 = tx
        .query_row(
            "SELECT synced_revision FROM task_progress_sync_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(store::error)?;
    if now.generation != snapshot.generation
        || now.revision < snapshot.revision
        || synced_revision > snapshot.revision
    {
        return Ok(false);
    }
    if let Some(etag) = etag {
        record_seen(&tx, &snapshot.epoch)?;
        tx.execute(
            "UPDATE task_progress_sync_state SET etag=?1 WHERE id=1",
            [etag],
        )
        .map_err(store::error)?;
    } else if store::read_in_transaction(&tx)?.etag.is_some() || remote_seen(&tx, &snapshot.epoch)?
    {
        return Err("PROGRESS_REMOTE_MISSING".into());
    }
    // A late local edit remains dirty, but the actually sent operations are published.
    // Epoch/generation checks above prohibit receipts from a previous target/session.
    tx.execute(
        "UPDATE task_progress_operations SET published=1 WHERE write_revision<=?1 AND published=0",
        [snapshot.revision],
    )
    .map_err(store::error)?;
    let matches = now == *snapshot;
    if matches {
        tx.execute(
            "UPDATE task_progress_sync_state SET synced_revision=?1 WHERE id=1",
            params![snapshot.revision],
        )
        .map_err(store::error)?;
    }
    tx.commit().map_err(store::error)?;
    Ok(matches)
}

#[cfg(test)]
#[path = "task_progress_sync_tests.rs"]
mod tests;
