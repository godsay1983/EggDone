//! Internal storage foundation. UI commands, transport and backup integration are separate phases.
use crate::task_checklist_protocol::{valid_uuid, MAX_CLOCK};
use crate::task_template_protocol::{
    self as protocol, TaskTemplate, TemplateContent, TemplatesDocument,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateWrite {
    pub operation_uuid: String,
    pub uuid: String,
    pub expected: Option<TaskTemplate>,
    pub content: TemplateContent,
    pub deleted: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub document: TemplatesDocument,
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub generation: i64,
}
fn error(e: rusqlite::Error) -> String {
    format!("TEMPLATE_DATABASE: {e}")
}
pub fn read_in_transaction(tx: &Transaction<'_>) -> Result<Snapshot, String> {
    let mut s = tx.query_row("SELECT revision,synced_revision,etag,generation FROM task_template_sync_state WHERE id=1", [],
        |r| Ok(Snapshot { document: TemplatesDocument::default(), revision:r.get(0)?, synced_revision:r.get(1)?, etag:r.get(2)?, generation:r.get(3)? })).map_err(error)?;
    if !(0..=MAX_CLOCK).contains(&s.revision)
        || !(0..=s.revision).contains(&s.synced_revision)
        || !(0..=MAX_CLOCK).contains(&s.generation)
    {
        return Err("TEMPLATE_STATE_INVALID".into());
    }
    let mut query = tx
        .prepare("SELECT uuid,active,record_json FROM task_templates ORDER BY uuid")
        .map_err(error)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(error)?;
    for row in rows {
        let (id, active, raw) = row.map_err(error)?;
        let doc = protocol::parse(&format!("{{\"format_version\":1,\"templates\":[{raw}]}}"))?;
        let r = &doc.templates[0];
        if id != r.uuid || active != i64::from(r.deleted_at.is_none()) {
            return Err("TEMPLATE_INDEX_INVALID".into());
        }
        s.document.templates.extend(doc.templates);
    }
    protocol::encode(&s.document)?;
    Ok(s)
}
pub fn snapshot(db: &mut Connection) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
fn put(tx: &Transaction<'_>, row: &TaskTemplate) -> Result<(), String> {
    protocol::validate(row)?;
    let raw = serde_json::to_string(row).map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO task_templates(uuid,active,record_json) VALUES(?1,?2,?3)
        ON CONFLICT(uuid) DO UPDATE SET active=excluded.active,record_json=excluded.record_json WHERE record_json<>excluded.record_json",
        params![row.uuid,row.deleted_at.is_none(),raw]).map_err(error)?;
    Ok(())
}
/// Caller must roll back after any error. No transport or ACK is implied by merging records.
pub fn merge_in_transaction(
    tx: &Transaction<'_>,
    remote: &TemplatesDocument,
) -> Result<(), String> {
    let merged = protocol::merge(&read_in_transaction(tx)?.document, remote)?;
    for row in merged.templates {
        put(tx, &row)?;
    }
    Ok(())
}
pub fn merge(db: &mut Connection, remote: &TemplatesDocument) -> Result<Snapshot, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    merge_in_transaction(&tx, remote)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
pub fn save(
    db: &mut Connection,
    r: &TemplateWrite,
    now: i64,
    by: &str,
) -> Result<TaskTemplate, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let result = save_in_transaction(&tx, r, now, by)?;
    tx.commit().map_err(error)?;
    Ok(result)
}
pub fn save_in_transaction(
    tx: &Transaction<'_>,
    r: &TemplateWrite,
    now: i64,
    by: &str,
) -> Result<TaskTemplate, String> {
    protocol::validate_content(&r.content)?;
    if !valid_uuid(&r.uuid)
        || !valid_uuid(&r.operation_uuid)
        || !(0..=MAX_CLOCK).contains(&now)
        || by.is_empty()
        || by.len() > 128
        || !by
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err("INVALID_TEMPLATE_WRITE".into());
    }
    if let Some(expected) = &r.expected {
        protocol::validate(expected)?;
        if expected.uuid != r.uuid {
            return Err("INVALID_TEMPLATE_WRITE".into());
        }
    }
    let payload = serde_json::to_string(&(r, by)).map_err(|e| e.to_string())?;
    let s = read_in_transaction(tx)?;
    let old = s.document.templates.iter().find(|i| i.uuid == r.uuid);
    let receipt: Option<(String, String)> = tx
        .query_row(
            "SELECT payload,result_json FROM task_template_operations WHERE operation_uuid=?1",
            [&r.operation_uuid],
            |q| Ok((q.get(0)?, q.get(1)?)),
        )
        .optional()
        .map_err(error)?;
    if let Some((previous, raw)) = receipt {
        if previous != payload {
            return Err("TEMPLATE_OPERATION_REUSED".into());
        }
        let result = protocol::parse(&format!("{{\"format_version\":1,\"templates\":[{raw}]}}"))?
            .templates
            .remove(0);
        if old != Some(&result) {
            return Err("TEMPLATE_STALE_RECEIPT".into());
        }
        return Ok(result);
    }
    if old != r.expected.as_ref() {
        return Err("TEMPLATE_STALE_DRAFT".into());
    }
    if old.is_some_and(|i| i.deleted_at.is_some()) {
        return Err("TEMPLATE_DELETED".into());
    }
    if r.deleted && (old.is_none() || old.is_some_and(|i| i.content != r.content)) {
        return Err("INVALID_TEMPLATE_DELETE".into());
    }
    if old.is_none()
        && s.document
            .templates
            .iter()
            .filter(|i| i.deleted_at.is_none())
            .count()
            >= 100
    {
        return Err("TEMPLATE_LIMIT".into());
    }
    let old_count = old.map_or(0, |i| i.content.checklist.len());
    if r.content.checklist.len() > 20 && r.content.checklist.len() > old_count {
        return Err("TEMPLATE_CHECKLIST_LIMIT".into());
    }
    if !r.deleted && old.map(|i| &i.content.group_uuid) != Some(&r.content.group_uuid) {
        if let Some(group) = &r.content.group_uuid {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid=?1 AND deleted_at IS NULL)",
                    [group],
                    |q| q.get(0),
                )
                .map_err(error)?;
            if !exists {
                return Err("TEMPLATE_GROUP_MISSING".into());
            }
        }
    }
    let time = old.map_or(now, |i| now.max(i.updated_at.saturating_add(1)));
    if time > MAX_CLOCK {
        return Err("TEMPLATE_CLOCK_OVERFLOW".into());
    }
    let row = TaskTemplate {
        uuid: r.uuid.clone(),
        content: r.content.clone(),
        created_at: old.map_or(time, |i| i.created_at),
        updated_at: time,
        updated_by: by.into(),
        deleted_at: r.deleted.then_some(time),
    };
    put(tx, &row)?;
    // Revalidate the whole document before the receipt, including global wire limits.
    read_in_transaction(tx)?;
    let result = serde_json::to_string(&row).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO task_template_operations(operation_uuid,payload,result_json) VALUES(?1,?2,?3)",
        params![r.operation_uuid, payload, result],
    )
    .map_err(error)?;
    Ok(row)
}
