//! Internal custom-rule transaction API. UI and S3 integration remain gated.
use crate::recurrence::{recurrence_occurrence_key, RecurrenceOccurrence};
use crate::recurrence_progress::{
    plan_recurrence_advance, CompletionEvidence, RecurrenceAdvancePlan,
};
use crate::recurrence_protocol::{validate_rule, RecurrenceRule};
use crate::recurrence_store;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrenceAction {
    Complete,
    Skip,
    Reconcile,
}

pub struct AdvanceRequest<'a> {
    pub rule_uuid: &'a str,
    pub current_todo_uuid: &'a str,
    pub action: RecurrenceAction,
    pub now: i64,
    pub device_id: &'a str,
    pub reminder_at: Option<i64>,
}

#[derive(Debug)]
struct Source {
    evidence: CompletionEvidence,
    updated_at: i64,
    due_at: Option<i64>,
    reminder_at: Option<i64>,
    repeat_rule: Option<String>,
    series: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceReceipt {
    rule_uuid: String,
    occurrence_key: String,
    planned_date: String,
    planned_due_at: Option<i64>,
    projected_due_at: Option<i64>,
}

fn db_error(error: rusqlite::Error) -> String {
    format!("RECURRENCE_DATABASE: {error}")
}

fn source(connection: &Connection, uuid: &str) -> Result<Option<Source>, String> {
    connection
        .query_row(
            "SELECT uuid,completed,deleted_at,archived_at,updated_at,due_at,reminder_at,
        repeat_rule,repeat_series_uuid FROM todos WHERE uuid=?1",
            [uuid],
            |row| {
                Ok(Source {
                    evidence: CompletionEvidence {
                        uuid: row.get(0)?,
                        completed: row.get(1)?,
                        deleted_at: row.get(2)?,
                        archived_at: row.get(3)?,
                    },
                    updated_at: row.get(4)?,
                    due_at: row.get(5)?,
                    reminder_at: row.get(6)?,
                    repeat_rule: row.get(7)?,
                    series: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(db_error)
}

fn inherited_reminder(source: &Source, next_due: Option<i64>, now: i64) -> Option<i64> {
    let value = next_due?.checked_add(source.reminder_at?.checked_sub(source.due_at?)?)?;
    (value > now && (0..=MAX_SAFE).contains(&value)).then_some(value)
}

pub fn advance_current(
    connection: &mut Connection,
    request: &AdvanceRequest<'_>,
) -> Result<Option<RecurrenceAdvancePlan>, String> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let result = advance_in_transaction(&tx, request)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}

// Reuses the transaction owned by the combined Todo/rule snapshot preparation.
pub(crate) fn advance_in_transaction(
    tx: &Transaction<'_>,
    request: &AdvanceRequest<'_>,
) -> Result<Option<RecurrenceAdvancePlan>, String> {
    if !(0..=MAX_SAFE).contains(&request.now)
        || request.reminder_at.is_some_and(|n| {
            !(1..=MAX_SAFE).contains(&n) || request.action != RecurrenceAction::Complete
        })
    {
        return Err("INVALID_RECURRENCE_ACTION".to_string());
    }
    let snapshot = recurrence_store::snapshot(&tx)?;
    let Some(rule) = snapshot
        .document
        .rules
        .iter()
        .find(|r| r.uuid == request.rule_uuid)
    else {
        return Ok(None);
    };
    if rule.deleted_at.is_some()
        || rule.exhausted
        || rule.current_todo_uuid != request.current_todo_uuid
    {
        return Ok(None);
    }
    let Some(mut current) = source(&tx, request.current_todo_uuid)? else {
        return Ok(None);
    };
    if request.action != RecurrenceAction::Reconcile
        && (current.evidence.deleted_at.is_some() || current.evidence.archived_at.is_some())
    {
        return Ok(None);
    }
    if current.repeat_rule.is_some()
        || current
            .series
            .as_deref()
            .is_some_and(|s| s != rule.first_todo_uuid)
        || (rule.generated_count > 1 && current.series.as_deref() != Some(&rule.first_todo_uuid))
    {
        return Err("RECURRENCE_LINK_CONFLICT".to_string());
    }
    let receipt_key = request
        .reminder_at
        .map(|n| format!("reminder.complete.v1:{}:{n}", request.current_todo_uuid));
    if let Some(key) = &receipt_key {
        let consumed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
                [key],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if consumed || current.reminder_at != request.reminder_at {
            return Ok(None);
        }
    }
    if !(0..MAX_SAFE).contains(&current.updated_at) || rule.updated_at == MAX_SAFE {
        return Err("INVALID_RECURRENCE_PROGRESS".to_string());
    }
    let now = request
        .now
        .max(current.updated_at + 1)
        .max(rule.updated_at + 1);
    let mut checked: RecurrenceRule = rule.clone();
    checked.updated_at = now;
    checked.updated_by = request.device_id.to_string();
    validate_rule(&checked)?;
    match request.action {
        RecurrenceAction::Complete if !current.evidence.completed => {
            tx.execute("UPDATE todos SET completed=1,completed_at=?1,updated_at=?1,updated_by=?2 WHERE uuid=?3",
                params![now, request.device_id, request.current_todo_uuid]).map_err(db_error)?;
            current.evidence.completed = true;
        }
        RecurrenceAction::Skip => {
            tx.execute(
                "UPDATE todos SET deleted_at=?1,updated_at=?1,updated_by=?2 WHERE uuid=?3",
                params![now, request.device_id, request.current_todo_uuid],
            )
            .map_err(db_error)?;
            current.evidence.deleted_at = Some(now);
        }
        _ => (),
    }
    let Some(plan) =
        plan_recurrence_advance(rule, Some(&current.evidence), now, request.device_id)?
    else {
        return Ok(None);
    };
    if let Some(next) = &plan.next {
        if next.due_at.is_some_and(|n| !(0..=MAX_SAFE).contains(&n)) {
            return Err("INVALID_RECURRENCE_TIME".to_string());
        }
        let instance_key = format!("recurrence.instance.v1:{}", next.uuid);
        let occurrence_key = recurrence_occurrence_key(
            &rule.uuid,
            &rule.schedule,
            &RecurrenceOccurrence {
                date: next.date.clone(),
                index: next.index,
            },
        )?;
        let existing_receipt: Option<String> = tx
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [&instance_key],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_error)?;
        if let Some(raw) = &existing_receipt {
            let receipt: InstanceReceipt =
                serde_json::from_str(raw).map_err(|_| "RECURRENCE_LINK_CONFLICT".to_string())?;
            if receipt.rule_uuid != rule.uuid || receipt.occurrence_key != occurrence_key {
                return Err("RECURRENCE_LINK_CONFLICT".to_string());
            }
        }
        if let Some(existing) = source(&tx, &next.uuid)? {
            if existing.repeat_rule.is_some()
                || existing.series.as_deref() != Some(&rule.first_todo_uuid)
            {
                return Err("RECURRENCE_LINK_CONFLICT".to_string());
            }
        } else {
            // A receipt without its Todo indicates a purged occurrence. Never resurrect it.
            if existing_receipt.is_some() {
                return Err("RECURRENCE_OCCURRENCE_MISSING".to_string());
            }
            let sort: i64 = tx
                .query_row(
                    "SELECT COALESCE(MIN(sort_order),1024)-1024 FROM todos
                WHERE deleted_at IS NULL AND archived_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if !(-MAX_SAFE..=MAX_SAFE).contains(&sort) {
                return Err("INVALID_RECURRENCE_SORT".to_string());
            }
            let reminder = inherited_reminder(&current, next.due_at, now);
            tx.execute("INSERT INTO todos(uuid,title,note,group_uuid,completed,pinned,priority,sort_order,
                created_at,updated_at,completed_at,deleted_at,archived_at,due_date,due_at,reminder_at,
                repeat_rule,repeat_next_due_date,repeat_series_uuid,updated_by)
                SELECT ?1,title,note,CASE WHEN EXISTS(SELECT 1 FROM groups WHERE uuid=todos.group_uuid AND deleted_at IS NULL)
                THEN group_uuid ELSE NULL END,0,pinned,priority,?2,?3,?3,NULL,NULL,NULL,?4,?5,?6,NULL,NULL,?7,?8
                FROM todos WHERE uuid=?9", params![next.uuid, sort, now,
                    if next.due_at.is_none() { Some(&next.date) } else { None }, next.due_at, reminder,
                    rule.first_todo_uuid, request.device_id, request.current_todo_uuid]).map_err(db_error)?;
        }
        if existing_receipt.is_none() {
            let receipt = InstanceReceipt {
                rule_uuid: rule.uuid.clone(),
                occurrence_key,
                planned_date: next.date.clone(),
                planned_due_at: next.due_at,
                projected_due_at: next.due_at,
            };
            let raw = serde_json::to_string(&receipt).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![instance_key, raw],
            )
            .map_err(db_error)?;
        }
    }
    let raw = serde_json::to_string(&plan.rule).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE recurrence_rules SET current_todo_uuid=?1,active=?2,record_json=?3 WHERE uuid=?4",
        params![
            plan.rule.current_todo_uuid,
            !plan.rule.exhausted,
            raw,
            rule.uuid
        ],
    )
    .map_err(db_error)?;
    recurrence_store::snapshot(&tx)?;
    if let Some(key) = receipt_key {
        tx.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![key, now.to_string()],
        )
        .map_err(db_error)?;
    }
    Ok(Some(plan))
}

#[cfg(test)]
#[path = "recurrence_transaction_tests.rs"]
mod tests;
