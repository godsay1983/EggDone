//! Independent template snapshots. A receipt acknowledges exactly one revision and target epoch.
use crate::{sync_target, task_template_protocol as protocol, task_template_store as store};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

pub fn object_key(todo: &str, occupied: &[String]) -> Result<String, String> {
    crate::task_note_link_protocol::object_key(todo, &[])?;
    let directory = todo.rfind('/').map_or("", |i| &todo[..=i]);
    let key = format!("{directory}task-templates.json");
    if key == todo || key.len() > 1024 || occupied.contains(&key) {
        return Err("TEMPLATE_KEY_COLLISION".into());
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
    "TEMPLATE_SYNC_DATABASE".into()
}
fn current(tx: &Transaction<'_>, epoch: &str) -> Result<bool, String> {
    Ok(!epoch.is_empty() && !epoch.starts_with("pending:") && sync_target::is_current(tx, epoch)?)
}
fn capture(tx: &Transaction<'_>, epoch: &str) -> Result<Snapshot, String> {
    let s = store::read_in_transaction(tx)?;
    Ok(Snapshot {
        document: protocol::encode(&s.document)?,
        revision: s.revision,
        generation: s.generation,
        epoch: epoch.into(),
    })
}
pub fn prepare(db: &mut Connection, epoch: &str, remote: &str) -> Result<Snapshot, String> {
    let document = protocol::parse(remote)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, epoch)? {
        return Err("TEMPLATE_CONFIG_CHANGED".into());
    }
    store::merge_in_transaction(&tx, &document)?;
    let snapshot = capture(&tx, epoch)?;
    tx.commit().map_err(error)?;
    Ok(snapshot)
}
pub fn is_current(db: &mut Connection, snapshot: &Snapshot) -> Result<bool, String> {
    check(db, snapshot, None)
}
pub fn acknowledge(
    db: &mut Connection,
    snapshot: &Snapshot,
    etag: Option<&str>,
) -> Result<bool, String> {
    if let Some(etag) = etag {
        if !crate::task_checklist_transport::valid_etag(etag) {
            return Err("TEMPLATE_ETAG_REQUIRED".into());
        }
    } else if !protocol::parse(&snapshot.document)?.templates.is_empty() {
        return Err("TEMPLATE_NOT_EMPTY".into());
    }
    check(db, snapshot, Some(etag))
}
fn check(
    db: &mut Connection,
    snapshot: &Snapshot,
    ack: Option<Option<&str>>,
) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, &snapshot.epoch)? || capture(&tx, &snapshot.epoch)? != *snapshot {
        return Ok(false);
    }
    if let Some(etag) = ack {
        tx.execute(
            "UPDATE task_template_sync_state SET synced_revision=?1,etag=?2 WHERE id=1",
            params![snapshot.revision, etag],
        )
        .map_err(error)?;
    }
    tx.commit().map_err(error)?;
    Ok(true)
}

#[cfg(test)]
#[path = "task_template_sync_tests.rs"]
mod tests;
