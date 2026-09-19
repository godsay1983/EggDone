//! Independent workflow snapshots and exact revision/target receipts.
#[cfg(test)]
#[path = "task_workflow_sync_tests.rs"]
mod tests;
use crate::{sync_target, task_workflow_protocol as protocol, task_workflow_store as store};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

pub fn object_key(todo: &str, occupied: &[String]) -> Result<String, String> {
    crate::task_note_link_protocol::object_key(todo, &[])?;
    let key = format!(
        "eggdone-workflow/v1/{:x}/states.json",
        Sha256::digest(todo.as_bytes())
    );
    if key == todo || occupied.contains(&key) {
        return Err("WORKFLOW_KEY_COLLISION".into());
    }
    Ok(key)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub document: String,
    pub revision: i64,
    pub generation: i64,
    epoch: String,
}

fn error(_: rusqlite::Error) -> String {
    "WORKFLOW_SYNC_DATABASE".into()
}
pub(crate) fn remote_seen(db: &Connection, epoch: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
        [format!("task.workflow.remote.seen.v1:{epoch}")],
        |r| r.get(0),
    )
    .map_err(error)
}
fn record_seen(db: &Connection, epoch: &str) -> Result<(), String> {
    db.execute(
        "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,'1')",
        [format!("task.workflow.remote.seen.v1:{epoch}")],
    )
    .map_err(error)?;
    Ok(())
}
fn current(tx: &Transaction<'_>, epoch: &str) -> Result<bool, String> {
    Ok(!epoch.is_empty() && !epoch.starts_with("pending:") && sync_target::is_current(tx, epoch)?)
}
fn capture(tx: &Transaction<'_>, epoch: &str) -> Result<Snapshot, String> {
    let state = store::read_in_transaction(tx)?;
    Ok(Snapshot {
        document: protocol::encode(&state.document)?,
        revision: state.revision,
        generation: state.generation,
        epoch: epoch.into(),
    })
}

pub fn prepare(
    db: &mut Connection,
    epoch: &str,
    remote: Option<&str>,
    etag: Option<&str>,
) -> Result<Snapshot, String> {
    if remote.is_some() != etag.is_some()
        || etag.is_some_and(|value| !crate::task_checklist_transport::valid_etag(value))
    {
        return Err("WORKFLOW_ETAG_REQUIRED".into());
    }
    let document = remote.map(protocol::parse).transpose()?.unwrap_or_default();
    // Preserve validated GET existence even if a later merge fails and rolls back.
    if !sync_target::is_current(db, epoch)? {
        return Err("WORKFLOW_CONFIG_CHANGED".into());
    }
    if remote.is_some() {
        record_seen(db, epoch)?;
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, epoch)? {
        return Err("WORKFLOW_CONFIG_CHANGED".into());
    }
    if remote.is_none()
        && (store::read_in_transaction(&tx)?.etag.is_some() || remote_seen(&tx, epoch)?)
    {
        return Err("WORKFLOW_REMOTE_MISSING".into());
    }
    // Remember a validated GET even if its subsequent conditional upload conflicts.
    // This is existence evidence only, never an ACK or an upload precondition.
    if let Some(etag) = etag {
        tx.execute(
            "UPDATE task_workflow_sync_state SET etag=?1 WHERE id=1",
            [etag],
        )
        .map_err(error)?;
    }
    store::merge_in_transaction(&tx, &document)?;
    let snapshot = capture(&tx, epoch)?;
    tx.commit().map_err(error)?;
    Ok(snapshot)
}

pub fn is_current(db: &mut Connection, snapshot: &Snapshot) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    Ok(current(&tx, &snapshot.epoch)? && capture(&tx, &snapshot.epoch)? == *snapshot)
}

pub fn acknowledge(
    db: &mut Connection,
    snapshot: &Snapshot,
    etag: Option<&str>,
) -> Result<bool, String> {
    if let Some(etag) = etag {
        if !crate::task_checklist_transport::valid_etag(etag) {
            return Err("WORKFLOW_ETAG_REQUIRED".into());
        }
    } else if !protocol::is_empty(&protocol::parse(&snapshot.document)?) {
        return Err("WORKFLOW_NOT_EMPTY".into());
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, &snapshot.epoch)? {
        return Ok(false);
    }
    let now = capture(&tx, &snapshot.epoch)?;
    if now.generation != snapshot.generation {
        return Ok(false);
    }
    // A successful create must remain known even when a concurrent local edit prevents ACK.
    if let Some(etag) = etag {
        record_seen(&tx, &snapshot.epoch)?;
        tx.execute(
            "UPDATE task_workflow_sync_state SET etag=?1 WHERE id=1",
            [etag],
        )
        .map_err(error)?;
    } else if store::read_in_transaction(&tx)?.etag.is_some() || remote_seen(&tx, &snapshot.epoch)?
    {
        return Err("WORKFLOW_REMOTE_MISSING".into());
    }
    let matches = now == *snapshot;
    if matches {
        tx.execute(
            "UPDATE task_workflow_sync_state SET synced_revision=?1 WHERE id=1",
            params![snapshot.revision],
        )
        .map_err(error)?;
    }
    tx.commit().map_err(error)?;
    Ok(matches)
}
