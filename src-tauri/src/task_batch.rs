//! Local batch receipts share the existing metadata store; tasks use the normal sync triggers.
use crate::task_checklist_protocol::{valid_text, valid_uuid, MAX_CLOCK};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchItem {
    pub uuid: String,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub operation_uuid: String,
    pub group_uuid: Option<String>,
    pub items: Vec<BatchItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchResult {
    pub operation_uuid: String,
    pub task_uuids: Vec<String>,
    pub created_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    fingerprint: String,
    result: BatchResult,
    sort_orders: Vec<i64>,
}
fn invalid() -> String {
    "INVALID_BATCH_REQUEST".into()
}
fn db_error(e: rusqlite::Error) -> String {
    format!("BATCH_DATABASE: {e}")
}
pub fn validate(r: &BatchRequest) -> Result<(), String> {
    if !valid_uuid(&r.operation_uuid)
        || r.group_uuid.as_deref().is_some_and(|g| !valid_uuid(g))
        || r.items.is_empty()
        || r.items.len() > 50
    {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    for item in &r.items {
        if !valid_uuid(&item.uuid)
            || !ids.insert(&item.uuid)
            || item.title.is_empty()
            || item.title.trim() != item.title
            || item.title.starts_with('\u{feff}')
            || item.title.ends_with('\u{feff}')
            || !valid_text(&item.title, 100, false)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub fn parse(raw: &str) -> Result<BatchRequest, String> {
    if raw.encode_utf16().count() > 65536 {
        return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    let keys = value.as_object().ok_or_else(invalid)?;
    if keys.len() != 3
        || !["operation_uuid", "group_uuid", "items"]
            .iter()
            .all(|k| keys.contains_key(*k))
    {
        return Err(invalid());
    }
    let r: BatchRequest = serde_json::from_value(value).map_err(|_| invalid())?;
    validate(&r)?;
    Ok(r)
}
fn dependencies_exist(tx: &Transaction<'_>, id: &str) -> Result<bool, String> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_checklist_items WHERE todo_uuid=?1
        UNION ALL SELECT 1 FROM task_note_links WHERE todo_uuid=?1
        UNION ALL SELECT 1 FROM recurrence_rules WHERE current_todo_uuid=?1
        UNION ALL SELECT 1 FROM todos WHERE repeat_series_uuid=?1)",
        [id],
        |r| r.get(0),
    )
    .map_err(db_error)
}
fn group_exists(tx: &Transaction<'_>, group: Option<&str>) -> Result<bool, String> {
    match group {
        None => Ok(true),
        Some(g) => tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid=?1 AND deleted_at IS NULL)",
                [g],
                |r| r.get(0),
            )
            .map_err(db_error),
    }
}
pub fn create(
    db: &mut Connection,
    request: &BatchRequest,
    now: i64,
    by: &str,
) -> Result<BatchResult, String> {
    validate(request)?;
    if !(0..=MAX_CLOCK).contains(&now) || !valid_uuid(by) {
        return Err(invalid());
    }
    let key = format!("task.batch.create.v1:{}", request.operation_uuid);
    let fingerprint = serde_json::to_string(&(request, by)).map_err(|_| invalid())?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let saved: Option<String> = tx
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db_error)?;
    let rules = crate::recurrence_store::snapshot(&tx)?.document;
    let definitions = crate::task_checklist_store::read_in_transaction(&tx)?.definitions;
    if let Some(raw) = saved {
        let receipt: Receipt =
            serde_json::from_str(&raw).map_err(|_| "BATCH_INVALID_RECEIPT".to_string())?;
        if receipt.fingerprint != fingerprint {
            return Err("BATCH_OPERATION_REUSED".into());
        }
        if receipt.result.operation_uuid != request.operation_uuid
            || receipt.result.task_uuids
                != request
                    .items
                    .iter()
                    .map(|i| i.uuid.clone())
                    .collect::<Vec<_>>()
            || receipt.sort_orders.len() != request.items.len()
            || !(0..=MAX_CLOCK).contains(&receipt.result.created_at)
            || receipt
                .sort_orders
                .iter()
                .any(|n| !(-MAX_CLOCK..=MAX_CLOCK).contains(n))
        {
            return Err("BATCH_INVALID_RECEIPT".into());
        }
        if !group_exists(&tx, request.group_uuid.as_deref())? {
            return Err("BATCH_STALE_RECEIPT".into());
        }
        for (i, item) in request.items.iter().enumerate() {
            let unchanged: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM todos WHERE uuid=?1 AND title=?2
                AND note='' AND group_uuid IS ?3 AND created_at=?4 AND updated_at=?4 AND updated_by=?5 AND sort_order=?6
                AND completed=0 AND pinned=0 AND priority=0 AND completed_at IS NULL AND deleted_at IS NULL AND archived_at IS NULL
                AND due_date IS NULL AND due_at IS NULL AND reminder_at IS NULL AND repeat_rule IS NULL
                AND repeat_series_uuid IS NULL AND repeat_next_due_date IS NULL)",
                params![item.uuid,item.title,request.group_uuid,receipt.result.created_at,by,receipt.sort_orders[i]], |r| r.get(0)).map_err(db_error)?;
            if !unchanged
                || dependencies_exist(&tx, &item.uuid)?
                || rules.rules.iter().any(|r| r.first_todo_uuid == item.uuid)
                || definitions
                    .definitions
                    .iter()
                    .any(|d| d.first_todo_uuid == item.uuid)
            {
                return Err("BATCH_STALE_RECEIPT".into());
            }
        }
        tx.commit().map_err(db_error)?;
        return Ok(receipt.result);
    }
    if !group_exists(&tx, request.group_uuid.as_deref())? {
        return Err("BATCH_GROUP_MISSING".into());
    }
    // Reject orphan identities before inserting: late sync must not attach old state to a new task.
    for item in &request.items {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM todos WHERE uuid=?1)",
                [&item.uuid],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if exists
            || dependencies_exist(&tx, &item.uuid)?
            || rules.rules.iter().any(|r| r.first_todo_uuid == item.uuid)
            || definitions
                .definitions
                .iter()
                .any(|d| d.first_todo_uuid == item.uuid)
        {
            return Err("BATCH_IDENTITY_EXISTS".into());
        }
    }
    let minimum: i64 = tx.query_row("SELECT COALESCE(MIN(sort_order),1024) FROM todos WHERE deleted_at IS NULL AND completed=0", [], |r| r.get(0)).map_err(db_error)?;
    let start = minimum
        .checked_sub(request.items.len() as i64 * 1024)
        .ok_or("BATCH_ORDER_OVERFLOW")?;
    if !(-MAX_CLOCK..=MAX_CLOCK).contains(&minimum) || start < -MAX_CLOCK {
        return Err("BATCH_ORDER_OVERFLOW".into());
    }
    let mut orders = Vec::new();
    for (i, item) in request.items.iter().enumerate() {
        let order = start + i as i64 * 1024;
        tx.execute("INSERT INTO todos(uuid,title,note,group_uuid,sort_order,created_at,updated_at,updated_by)
            VALUES(?1,?2,'',?3,?4,?5,?5,?6)", params![item.uuid,item.title,request.group_uuid,order,now,by]).map_err(db_error)?;
        orders.push(order);
    }
    let result = BatchResult {
        operation_uuid: request.operation_uuid.clone(),
        task_uuids: request.items.iter().map(|i| i.uuid.clone()).collect(),
        created_at: now,
    };
    let receipt = Receipt {
        fingerprint,
        result: result.clone(),
        sort_orders: orders,
    };
    tx.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![key, serde_json::to_string(&receipt).map_err(|_| invalid())?],
    )
    .map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}
