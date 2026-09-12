//! L1 persistence only; not connected to UI, entity mutations or network transport.
use crate::task_note_link_protocol::{
    encode_document, merge_documents, parse_document, LinkDocument,
};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

#[derive(Debug)]
pub struct LinkSnapshot {
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub document: LinkDocument,
}

fn db_error(e: rusqlite::Error) -> String {
    format!("TASK_NOTE_LINK_DATABASE: {e}")
}

pub fn snapshot(db: &Connection) -> Result<LinkSnapshot, String> {
    let (revision, synced_revision, etag) = db
        .query_row(
            "SELECT revision,synced_revision,etag FROM task_note_link_sync_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(db_error)?;
    let mut statement = db
        .prepare("SELECT record_json FROM task_note_links ORDER BY uuid")
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(db_error)?;
    let mut document = LinkDocument {
        format_version: 1,
        links: Vec::new(),
    };
    for row in rows {
        let source = format!(
            "{{\"format_version\":1,\"links\":[{}]}}",
            row.map_err(db_error)?
        );
        document.links.extend(parse_document(&source)?.links);
    }
    encode_document(&document)?;
    Ok(LinkSnapshot {
        revision,
        synced_revision,
        etag,
        document,
    })
}

pub fn merge(db: &mut Connection, incoming: &LinkDocument) -> Result<LinkSnapshot, String> {
    encode_document(incoming)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    merge_in_transaction(&tx, incoming)?;
    let result = snapshot(&tx)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}

pub fn merge_in_transaction(tx: &Transaction<'_>, incoming: &LinkDocument) -> Result<(), String> {
    merge_in_open_transaction(tx, incoming)
}

fn merge_in_open_transaction(tx: &Connection, incoming: &LinkDocument) -> Result<(), String> {
    if tx.is_autocommit() {
        return Err("TASK_NOTE_LINK_TRANSACTION_REQUIRED".into());
    }
    let current = snapshot(tx)?;
    let merged = merge_documents(&current.document, incoming)?;
    for link in merged.links {
        let record = serde_json::to_string(&link).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO task_note_links(uuid,todo_uuid,note_uuid,active,record_json) VALUES (?1,?2,?3,?4,?5)
          ON CONFLICT(uuid) DO UPDATE SET active=excluded.active,record_json=excluded.record_json
          WHERE record_json<>excluded.record_json",
          params![link.uuid, link.todo_uuid, link.note_uuid, link.deleted_at.is_none(), record]).map_err(db_error)?;
    }
    Ok(())
}

pub(crate) fn tombstone_entity(
    tx: &Connection,
    uuid: &str,
    is_note: bool,
    now: i64,
    by: &str,
) -> Result<(), String> {
    if tx.is_autocommit() {
        return Err("TASK_NOTE_LINK_TRANSACTION_REQUIRED".into());
    }
    let mut links = snapshot(tx)?.document.links;
    links.retain(|l| {
        l.deleted_at.is_none()
            && if is_note {
                l.note_uuid == uuid
            } else {
                l.todo_uuid == uuid
            }
    });
    for link in &mut links {
        let stamp = now.max(link.updated_at.saturating_add(1));
        if !(0..=crate::task_note_link_protocol::MAX_CLOCK).contains(&stamp) {
            return Err("TASK_NOTE_LINK_CLOCK_EXHAUSTED".into());
        }
        link.updated_at = stamp;
        link.updated_by = by.into();
        link.deleted_at = Some(stamp);
    }
    if !links.is_empty() {
        merge_in_open_transaction(
            tx,
            &LinkDocument {
                format_version: 1,
                links,
            },
        )?;
    }
    Ok(())
}
