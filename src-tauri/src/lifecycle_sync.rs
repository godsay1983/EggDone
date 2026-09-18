//! Permanent-deletion evidence. Network callers must publish this ledger before body cleanup.
use crate::purge::{self, Terminal};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: u32,
    pub terminals: Vec<Terminal>,
}
pub fn parse(raw: &str) -> Result<Document, String> {
    if raw.len() > 4 * 1024 * 1024 {
        return Err("PURGE_LEDGER_LIMIT".into());
    }
    let mut value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "PURGE_LEDGER_INVALID")?;
    fn integer(value: &mut serde_json::Value) {
        if let Some(n) = value.as_f64() {
            if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
                *value = serde_json::json!(n as i64);
            }
        }
    }
    if let Some(version) = value.get_mut("format_version") {
        integer(version);
    }
    if let Some(rows) = value.get_mut("terminals").and_then(|v| v.as_array_mut()) {
        for row in rows {
            if let Some(time) = row.get_mut("purged_at") {
                integer(time);
            }
        }
    }
    let document: Document = serde_json::from_value(value).map_err(|_| "PURGE_LEDGER_INVALID")?;
    validate(&document)?;
    Ok(document)
}
fn validate(document: &Document) -> Result<(), String> {
    if document.format_version != 1 {
        return Err("PURGE_LEDGER_INVALID".into());
    }
    purge::validate_terminals(&document.terminals)
}
pub fn merge(a: &Document, b: &Document) -> Result<Document, String> {
    validate(a)?;
    validate(b)?;
    let mut records = BTreeMap::<(String, String), Terminal>::new();
    for record in a.terminals.iter().chain(&b.terminals) {
        let key = (record.kind.clone(), record.uuid.clone());
        if records.get(&key).is_none_or(|old| {
            (record.purged_at, &record.operation_uuid) > (old.purged_at, &old.operation_uuid)
        }) {
            records.insert(key, record.clone());
        }
    }
    let document = Document {
        format_version: 1,
        terminals: records.into_values().collect(),
    };
    encode(&document)?;
    Ok(document)
}
pub fn encode(document: &Document) -> Result<String, String> {
    validate(document)?;
    let raw = serde_json::to_string(document).map_err(|_| "PURGE_LEDGER_INVALID")?;
    if raw.len() > 4 * 1024 * 1024 {
        return Err("PURGE_LEDGER_LIMIT".into());
    }
    Ok(raw)
}
pub struct Index {
    todos: BTreeSet<String>,
    notes: BTreeSet<String>,
    attachments: BTreeSet<String>,
}
impl Index {
    pub fn read(c: &Connection) -> Result<Self, String> {
        let records = purge::terminals(c)?;
        let mut index = Self {
            todos: BTreeSet::new(),
            notes: BTreeSet::new(),
            attachments: BTreeSet::new(),
        };
        for record in records {
            if record.kind == "todo" {
                index.todos.insert(record.uuid);
            } else {
                index.notes.insert(record.uuid);
            }
        }
        let mut q = c
            .prepare("SELECT attachment_uuid FROM purge_cleanup")
            .map_err(db)?;
        for row in q.query_map([], |r| r.get::<_, String>(0)).map_err(db)? {
            index.attachments.insert(row.map_err(db)?);
        }
        Ok(index)
    }
    pub fn todo(&self, id: &str) -> bool {
        self.todos.contains(id)
    }
    pub fn note(&self, id: &str) -> bool {
        self.notes.contains(id)
    }
    pub fn attachment(&self, id: &str, note: &str) -> bool {
        self.attachments.contains(id) || self.note(note)
    }
}
fn db(_: rusqlite::Error) -> String {
    "PURGE_DATABASE_FAILED".into()
}
pub(crate) fn guard(c: &Connection, epoch: &str) -> Result<String, String> {
    if epoch.is_empty() || epoch.starts_with("pending:") {
        return Err("PURGE_TARGET_CHANGED".into());
    }
    if !crate::sync_target::is_current(c, epoch)? {
        return Err("PURGE_TARGET_CHANGED".into());
    }
    let key: String = c
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(db)?;
    let (_, domain) = crate::sync_space::scope(&key)?.ok_or("PURGE_MIGRATION_REQUIRED")?;
    if domain != "todos" {
        return Err("SYNC_SPACE_KEY".into());
    }
    Ok(key)
}
pub struct Snapshot {
    pub document: Document,
    pub revision: i64,
}

/// Only the synchronization session may apply authoritative remote terminals. Import is intentionally separate.
pub fn prepare(c: &mut Connection, epoch: &str, incoming: &Document) -> Result<Snapshot, String> {
    validate(incoming)?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    let snapshot = prepare_in_transaction(&tx, epoch, incoming)?;
    tx.commit().map_err(db)?;
    Ok(snapshot)
}

pub(crate) fn prepare_in_transaction(
    tx: &Connection,
    epoch: &str,
    incoming: &Document,
) -> Result<Snapshot, String> {
    validate(incoming)?;
    let key = guard(&tx, epoch)?;
    crate::purge_remote::bind_target(&tx, epoch)?;
    let local = Document {
        format_version: 1,
        terminals: purge::terminals(&tx)?,
    };
    let merged = merge(&local, incoming)?;
    for terminal in &merged.terminals {
        if terminal.kind == "todo" {
            // A stale rule pointing at a terminal task must not recreate an instance. Other instances/definitions survive.
            tx.execute(
                "DELETE FROM recurrence_rules WHERE current_todo_uuid=?1",
                [&terminal.uuid],
            )
            .map_err(db)?;
        } else {
            // Preserve immutable cleanup evidence in the same transaction before removing attachment metadata.
            let mut q = tx.prepare("SELECT uuid,kind,byte_size,sha256,preview_byte_size,preview_sha256,remote_uploaded FROM note_attachments WHERE note_uuid=?1").map_err(db)?;
            let rows = q
                .query_map([&terminal.uuid], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, i64>(6)?,
                    ))
                })
                .map_err(db)?;
            for row in rows {
                let (id, kind, size, hash, preview_size, preview_hash, uploaded) =
                    row.map_err(db)?;
                if uploaded == 0 {
                    continue;
                }
                let evidence = serde_json::json!([
                    1,
                    epoch,
                    key,
                    terminal.uuid,
                    id,
                    kind,
                    size,
                    hash,
                    preview_size,
                    preview_hash
                ]);
                tx.execute(
                    "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,?2)",
                    params![
                        format!("purge.remote.evidence.v1:{epoch}:{id}"),
                        evidence.to_string()
                    ],
                )
                .map_err(db)?;
            }
        }
        purge::erase(
            &tx,
            &purge::Target {
                kind: terminal.kind.clone(),
                uuid: terminal.uuid.clone(),
            },
            &terminal.operation_uuid,
            terminal.purged_at,
        )?;
        tx.execute("UPDATE lifecycle_terminals SET operation_uuid=?1,purged_at=?2 WHERE kind=?3 AND uuid=?4 AND (operation_uuid<>?1 OR purged_at<>?2)", params![terminal.operation_uuid, terminal.purged_at, terminal.kind, terminal.uuid]).map_err(db)?;
    }
    let revision = tx
        .query_row(
            "SELECT revision FROM lifecycle_sync_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(db)?;
    Ok(Snapshot {
        document: merged,
        revision,
    })
}
pub fn acknowledge(
    c: &mut Connection,
    epoch: &str,
    revision: i64,
    etag: &str,
) -> Result<bool, String> {
    if !(0..=9_007_199_254_740_991).contains(&revision)
        || !crate::migration_backup::cloud::valid_etag(etag)
    {
        return Err("PURGE_LEDGER_ACK".into());
    }
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    guard(&tx, epoch)?;
    let changed = tx
        .execute(
            "UPDATE lifecycle_sync_state SET synced_revision=?1,etag=?2 WHERE id=1 AND revision=?1",
            params![revision, etag],
        )
        .map_err(db)?;
    tx.commit().map_err(db)?;
    Ok(changed == 1)
}

#[cfg(test)]
#[path = "lifecycle_sync_tests.rs"]
mod tests;
