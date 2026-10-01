use crate::{
    task_checklist_protocol::valid_uuid,
    task_progress_protocol::{self as protocol, Document, Entry},
};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub document: Document,
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub generation: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressCursor {
    pub created_at: i64,
    pub uuid: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ProgressView {
    pub record: Entry,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ProgressPage {
    pub task_uuid: String,
    pub title: String,
    pub read_only: bool,
    pub total: i64,
    pub entries: Vec<ProgressView>,
    pub next_cursor: Option<ProgressCursor>,
    pub overwritten: bool,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ProgressCount {
    pub task_uuid: String,
    pub count: i64,
}
fn required_nullable<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressWrite {
    pub operation_uuid: String,
    pub task_uuid: String,
    pub record_uuid: String,
    pub action: String,
    pub body: String,
    #[serde(deserialize_with = "required_nullable")]
    pub expected_record: Option<String>,
}
pub(crate) fn error(e: rusqlite::Error) -> String {
    if let rusqlite::Error::SqliteFailure(_, Some(message)) = &e {
        if matches!(
            message.as_str(),
            "PROGRESS_LIMIT" | "PROGRESS_CONFLICT" | "PROGRESS_DELETED" | "PROGRESS_UNAVAILABLE"
        ) {
            return message.clone();
        }
    }
    "PROGRESS_DATABASE".into()
}
fn row(r: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        uuid: r.get(0)?,
        task_uuid: r.get(1)?,
        body: r.get(2)?,
        created_at: r.get(3)?,
        created_by: r.get(4)?,
        updated_at: r.get(5)?,
        updated_by: r.get(6)?,
        clock: r.get(7)?,
        deleted_at: r.get(8)?,
    })
}
pub fn record_token(e: &Entry) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(
            protocol::encode(&Document {
                format_version: 1,
                entries: vec![e.clone()]
            })?
            .as_bytes()
        )
    ))
}
pub fn read_in_transaction(db: &Connection) -> Result<Snapshot, String> {
    let mut state = db.query_row("SELECT revision,synced_revision,etag,generation FROM task_progress_sync_state WHERE id=1", [], |r| Ok(Snapshot {document:Document::default(),revision:r.get(0)?,synced_revision:r.get(1)?,etag:r.get(2)?,generation:r.get(3)?})).map_err(error)?;
    let mut q = db.prepare("SELECT uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock,deleted_at FROM task_progress_entries ORDER BY uuid").map_err(error)?;
    state.document.entries = q
        .query_map([], row)
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    protocol::validate(&state.document)?;
    Ok(state)
}
pub fn snapshot(db: &mut Connection) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
fn put(db: &Connection, e: &Entry) -> Result<(), String> {
    db.execute("INSERT INTO task_progress_entries(uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock,deleted_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(uuid) DO UPDATE SET body=excluded.body,updated_at=excluded.updated_at,updated_by=excluded.updated_by,clock=excluded.clock,deleted_at=excluded.deleted_at", params![e.uuid,e.task_uuid,e.body,e.created_at,e.created_by,e.updated_at,e.updated_by,e.clock,e.deleted_at]).map_err(error)?;
    Ok(())
}
fn purged(db: &Connection, task: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=?1)",
        [task],
        |r| r.get(0),
    )
    .map_err(error)
}
pub fn merge_in_transaction(db: &Connection, remote: &Document) -> Result<(), String> {
    protocol::validate(remote)?;
    let mut q = db
        .prepare("SELECT uuid FROM lifecycle_terminals WHERE kind='todo'")
        .map_err(error)?;
    let terminals = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(error)?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(error)?;
    let local = protocol::filter_purged(&read_in_transaction(db)?.document, &terminals)?;
    let remote = protocol::filter_purged(remote, &terminals)?;
    let merged = protocol::merge(&local, &remote)?;
    let prior: BTreeMap<_, _> = local.entries.iter().map(|e| (&e.uuid, e)).collect();
    // A notice belongs only to the exact unpublished local version displaced by merge.
    for e in &merged.entries {
        if let Some(old) = prior.get(&e.uuid) {
            if *old == e {
                continue;
            }
            let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM task_progress_operations WHERE record_uuid=?1 AND committed_token=?2 AND published=0)",params![e.uuid,record_token(old)?],|r|r.get(0)).map_err(error)?;
            if pending {
                db.execute(
                    "INSERT OR IGNORE INTO task_progress_notices(task_uuid) VALUES(?1)",
                    [&e.task_uuid],
                )
                .map_err(error)?;
            }
        }
        put(db, e)?;
    }
    db.execute("DELETE FROM task_progress_entries WHERE task_uuid IN(SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    db.execute("DELETE FROM task_progress_operations WHERE task_uuid IN(SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    db.execute("DELETE FROM task_progress_notices WHERE task_uuid IN(SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    read_in_transaction(db)?;
    Ok(())
}
pub fn restore(db: &mut Connection, remote: &Document) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    merge_in_transaction(&tx, remote)?;
    tx.commit().map_err(error)
}
pub fn list_in_transaction(
    db: &Connection,
    task: &str,
    cursor: Option<&ProgressCursor>,
) -> Result<ProgressPage, String> {
    if !valid_uuid(task)
        || cursor.is_some_and(|c| {
            !valid_uuid(&c.uuid) || !(0..=protocol::MAX_CLOCK).contains(&c.created_at)
        })
    {
        return Err("PROGRESS_INVALID".into());
    }
    if purged(db, task)? {
        return Err("PROGRESS_UNAVAILABLE".into());
    }
    let parent:Option<(String,bool)>=db.query_row("SELECT title,archived_at IS NOT NULL OR deleted_at IS NOT NULL FROM todos WHERE uuid=?1",[task],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(error)?;
    let (title, read_only) = parent.ok_or("PROGRESS_UNAVAILABLE")?;
    let total = db
        .query_row(
            "SELECT COUNT(*) FROM task_progress_entries WHERE task_uuid=?1 AND deleted_at IS NULL",
            [task],
            |r| r.get(0),
        )
        .map_err(error)?;
    let mut q=db.prepare("SELECT uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock,deleted_at FROM task_progress_entries WHERE task_uuid=?1 AND deleted_at IS NULL AND (?2 IS NULL OR created_at<?2 OR (created_at=?2 AND uuid<?3)) ORDER BY created_at DESC,uuid DESC LIMIT 31").map_err(error)?;
    let mut records: Vec<Entry> = q
        .query_map(
            params![
                task,
                cursor.map(|c| c.created_at),
                cursor.map(|c| c.uuid.as_str())
            ],
            row,
        )
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    let more = records.len() > 30;
    records.truncate(30);
    let next_cursor = if more {
        records.last().map(|e| ProgressCursor {
            created_at: e.created_at,
            uuid: e.uuid.clone(),
        })
    } else {
        None
    };
    let entries = records
        .into_iter()
        .map(|record| {
            Ok(ProgressView {
                token: record_token(&record)?,
                record,
            })
        })
        .collect::<Result<_, String>>()?;
    let overwritten = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM task_progress_notices WHERE task_uuid=?1)",
            [task],
            |r| r.get(0),
        )
        .map_err(error)?;
    Ok(ProgressPage {
        task_uuid: task.into(),
        title,
        read_only,
        total,
        entries,
        next_cursor,
        overwritten,
    })
}
pub fn list(
    db: &mut Connection,
    task: &str,
    cursor: Option<&ProgressCursor>,
) -> Result<ProgressPage, String> {
    let tx = db.transaction().map_err(error)?;
    let page = list_in_transaction(&tx, task, cursor)?;
    tx.commit().map_err(error)?;
    Ok(page)
}
pub fn counts(db: &Connection, tasks: &[String]) -> Result<Vec<ProgressCount>, String> {
    if tasks.len() > protocol::MAX_ROWS {
        return Err("PROGRESS_LIMIT".into());
    }
    if tasks.iter().any(|s| !valid_uuid(s)) {
        return Err("PROGRESS_INVALID".into());
    }
    let json = serde_json::to_string(tasks).map_err(|_| "PROGRESS_INVALID")?;
    let mut q=db.prepare("WITH requested AS (SELECT value AS uuid FROM json_each(?1)), totals AS (SELECT e.task_uuid,COUNT(*) AS n FROM task_progress_entries e JOIN todos t ON t.uuid=e.task_uuid WHERE e.deleted_at IS NULL AND e.task_uuid IN(SELECT uuid FROM requested) AND NOT EXISTS(SELECT 1 FROM lifecycle_terminals l WHERE l.kind='todo' AND l.uuid=e.task_uuid) GROUP BY e.task_uuid) SELECT r.uuid,COALESCE(t.n,0) FROM requested r LEFT JOIN totals t ON t.task_uuid=r.uuid").map_err(error)?;
    let result = q
        .query_map([json], |r| {
            Ok(ProgressCount {
                task_uuid: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    Ok(result)
}
pub fn dismiss_notice(db: &Connection, task: &str) -> Result<(), String> {
    if !valid_uuid(task) {
        return Err("PROGRESS_INVALID".into());
    }
    db.execute(
        "DELETE FROM task_progress_notices WHERE task_uuid=?1",
        [task],
    )
    .map_err(error)?;
    Ok(())
}
fn valid_token(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn normalize(r: &ProgressWrite) -> Result<ProgressWrite, String> {
    if !valid_uuid(&r.operation_uuid) || !valid_uuid(&r.task_uuid) || !valid_uuid(&r.record_uuid) {
        return Err("PROGRESS_INVALID".into());
    }
    let mut r = r.clone();
    match r.action.as_str() {
        "create" if r.expected_record.is_none() => {
            r.body = protocol::normalize_body(&r.body)?;
        }
        "edit" if r.expected_record.as_deref().is_some_and(valid_token) => {
            r.body = protocol::normalize_body(&r.body)?;
        }
        "delete" if r.body.is_empty() && r.expected_record.as_deref().is_some_and(valid_token) => {}
        _ => return Err("PROGRESS_INVALID".into()),
    }
    Ok(r)
}
// Caller supplies a transaction so records, revision and receipt commit together.
pub fn write_in_transaction(
    db: &Connection,
    r: &ProgressWrite,
    now: i64,
    by: &str,
) -> Result<ProgressPage, String> {
    let r = normalize(r)?;
    if purged(db, &r.task_uuid)? {
        return Err("PROGRESS_UNAVAILABLE".into());
    }
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&r).map_err(|_| "PROGRESS_INVALID")?)
    );
    let old: Option<String> = db
        .query_row(
            "SELECT request_digest FROM task_progress_operations WHERE operation_uuid=?1",
            [&r.operation_uuid],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    if let Some(old) = old {
        if old != digest {
            return Err("PROGRESS_CONFLICT".into());
        }
        return list_in_transaction(db, &r.task_uuid, None);
    }
    let page = list_in_transaction(db, &r.task_uuid, None)?;
    if page.read_only {
        return Err("PROGRESS_READ_ONLY".into());
    }
    if !(0..=protocol::MAX_CLOCK).contains(&now) {
        return Err("PROGRESS_INVALID".into());
    }
    let existing:Option<Entry>=db.query_row("SELECT uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock,deleted_at FROM task_progress_entries WHERE uuid=?1",[&r.record_uuid],row).optional().map_err(error)?;
    let entry = if r.action == "create" {
        if existing.is_some() {
            return Err("PROGRESS_CONFLICT".into());
        }
        Entry {
            uuid: r.record_uuid.clone(),
            task_uuid: r.task_uuid.clone(),
            body: r.body.clone(),
            created_at: now,
            created_by: by.into(),
            updated_at: now,
            updated_by: by.into(),
            clock: 1,
            deleted_at: None,
        }
    } else {
        let mut entry = existing.ok_or("PROGRESS_UNAVAILABLE")?;
        if entry.task_uuid != r.task_uuid {
            return Err("PROGRESS_CONFLICT".into());
        }
        if entry.deleted_at.is_some() {
            return Err("PROGRESS_DELETED".into());
        }
        if Some(record_token(&entry)?) != r.expected_record {
            return Err("PROGRESS_CONFLICT".into());
        }
        entry.clock = protocol::next_clock(entry.clock)?;
        entry.updated_at = now.max(entry.created_at).max(entry.updated_at);
        entry.updated_by = by.into();
        entry.body = r.body.clone();
        if r.action == "delete" {
            entry.deleted_at = Some(entry.updated_at);
        }
        entry
    };
    protocol::validate_entry(&entry)?;
    let mut document = read_in_transaction(db)?.document;
    document.entries.retain(|e| e.uuid != entry.uuid);
    document.entries.push(entry.clone());
    protocol::validate(&document)?;
    put(db, &entry)?;
    let revision: i64 = db
        .query_row(
            "SELECT revision FROM task_progress_sync_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(error)?;
    db.execute("INSERT INTO task_progress_operations(operation_uuid,request_digest,task_uuid,record_uuid,committed_token,write_revision) VALUES(?1,?2,?3,?4,?5,?6)",params![r.operation_uuid,digest,r.task_uuid,r.record_uuid,record_token(&entry)?,revision]).map_err(error)?;
    list_in_transaction(db, &r.task_uuid, None)
}
pub fn write(
    db: &mut Connection,
    r: &ProgressWrite,
    now: i64,
    by: &str,
) -> Result<ProgressPage, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let page = write_in_transaction(&tx, r, now, by)?;
    tx.commit().map_err(error)?;
    Ok(page)
}

#[cfg(test)]
#[path = "task_progress_store_tests.rs"]
mod tests;
