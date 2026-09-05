//! Internal rule editing transactions. UI and backup integration remain release gates.
use crate::recurrence_protocol::{
    encode_document, validate_rule, RecurrenceDocument, RecurrenceRule,
};
use crate::{recurrence_store, recurrence_time};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleEditRequest {
    pub rule: RecurrenceRule,
    pub expected_todo_updated_at: i64,
    pub replaces: Option<RecurrenceRule>,
}

fn db_error(error: rusqlite::Error) -> String {
    format!("RECURRENCE_DATABASE: {error}")
}

fn stamp(now: i64, previous: i64) -> Result<i64, String> {
    if !(0..=MAX_SAFE).contains(&now) || !(0..MAX_SAFE).contains(&previous) {
        return Err("INVALID_RECURRENCE_PROGRESS".into());
    }
    Ok(now.max(previous + 1))
}

// Callers retain this request (including UUID) when retrying an uncertain local result.
pub fn save_rule(
    connection: &mut Connection,
    request: &RuleEditRequest,
) -> Result<RecurrenceRule, String> {
    let draft = &request.rule;
    validate_rule(draft)?;
    // This operation writes Todo.updated_by, whose wire contract is stricter than rule metadata.
    if !uuid::Uuid::parse_str(&draft.updated_by)
        .is_ok_and(|id| id.to_string().eq_ignore_ascii_case(&draft.updated_by))
    {
        return Err("INVALID_RECURRENCE_EDIT".into());
    }
    if draft.generated_count != 1
        || draft.exhausted
        || draft.deleted_at.is_some()
        || !(0..=MAX_SAFE).contains(&request.expected_todo_updated_at)
    {
        return Err("INVALID_RECURRENCE_EDIT".into());
    }
    if let Some(old) = &request.replaces {
        validate_rule(old)?;
        if old.uuid == draft.uuid
            || old.deleted_at.is_some()
            || old.exhausted
            || old.current_todo_uuid != draft.first_todo_uuid
        {
            return Err("INVALID_RECURRENCE_EDIT".into());
        }
    }
    let fingerprint = serde_json::to_string(request).map_err(|_| "INVALID_RECURRENCE_EDIT")?;
    let receipt_key = format!("recurrence.edit.v1:{}", draft.uuid);
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let snapshot = recurrence_store::snapshot(&tx)?;
    let receipt: Option<String> = tx
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [&receipt_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if let Some(receipt) = receipt {
        if receipt != fingerprint {
            return Err("RECURRENCE_EDIT_CONFLICT".into());
        }
        return snapshot
            .document
            .rules
            .into_iter()
            .find(|r| r.uuid == draft.uuid)
            .ok_or_else(|| "RECURRENCE_EDIT_CONFLICT".into());
    }
    if snapshot.document.rules.iter().any(|r| r.uuid == draft.uuid) {
        return Err("RECURRENCE_EDIT_CONFLICT".into());
    }
    let task: Option<(i64, bool, Option<i64>, Option<i64>, Option<String>, Option<String>)> = tx.query_row(
        "SELECT updated_at,completed,deleted_at,archived_at,repeat_rule,repeat_series_uuid FROM todos WHERE uuid=?1",
        [&draft.first_todo_uuid], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))
        .optional().map_err(db_error)?;
    let Some((updated, completed, deleted, archived, legacy, series)) = task else {
        return Err("RECURRENCE_EDIT_CONFLICT".into());
    };
    if updated != request.expected_todo_updated_at
        || completed
        || deleted.is_some()
        || archived.is_some()
    {
        return Err("RECURRENCE_EDIT_CONFLICT".into());
    }
    if legacy.is_some() {
        return Err("RECURRENCE_LINK_CONFLICT".into());
    }
    let mut rules = snapshot.document.rules;
    let mut now = stamp(draft.updated_at, updated)?;
    match &request.replaces {
        Some(expected) => {
            let old = rules
                .iter_mut()
                .find(|r| r.uuid == expected.uuid)
                .ok_or("RECURRENCE_EDIT_CONFLICT")?;
            if old != expected {
                return Err("RECURRENCE_EDIT_CONFLICT".into());
            }
            if series.as_deref().is_some_and(|s| s != old.first_todo_uuid)
                || (series.is_none() && old.generated_count != 1)
            {
                return Err("RECURRENCE_LINK_CONFLICT".into());
            }
            now = stamp(now, old.updated_at)?;
            old.updated_at = now;
            old.updated_by = draft.updated_by.clone();
            old.deleted_at = Some(now);
        }
        None => {
            // A historical or partially synchronized series must not silently become a new one.
            if series.is_some()
                || rules.iter().any(|r| {
                    r.first_todo_uuid == draft.first_todo_uuid
                        || r.current_todo_uuid == draft.first_todo_uuid
                })
            {
                return Err("RECURRENCE_LINK_CONFLICT".into());
            }
        }
    }
    if rules.iter().any(|r| {
        r.deleted_at.is_none()
            && !r.exhausted
            && (r.first_todo_uuid == draft.first_todo_uuid
                || r.current_todo_uuid == draft.first_todo_uuid)
    }) {
        return Err("RECURRENCE_LINK_CONFLICT".into());
    }
    let due = recurrence_time::recurrence_due_at(
        &draft.current_date,
        draft.schedule.local_time_minutes,
        draft.timezone_id.as_deref(),
    )?;
    if due.is_some_and(|n| !(0..=MAX_SAFE).contains(&n)) {
        return Err("INVALID_RECURRENCE_TIME".into());
    }
    let mut saved = draft.clone();
    saved.updated_at = now;
    rules.push(saved.clone());
    encode_document(&RecurrenceDocument {
        format_version: 1,
        rules: rules.clone(),
    })?;
    if let Some(expected) = &request.replaces {
        let old = rules
            .iter()
            .find(|r| r.uuid == expected.uuid)
            .ok_or("RECURRENCE_EDIT_CONFLICT")?;
        tx.execute(
            "UPDATE recurrence_rules SET active=0,record_json=?1 WHERE uuid=?2",
            params![
                serde_json::to_string(old).map_err(|_| "INVALID_RECURRENCE_EDIT")?,
                old.uuid
            ],
        )
        .map_err(db_error)?;
    }
    // A changed schedule explicitly changes the current deadline; an old reminder is not reused.
    tx.execute(
        "UPDATE todos SET due_date=?1,due_at=?2,reminder_at=NULL,repeat_rule=NULL,
        repeat_next_due_date=NULL,repeat_series_uuid=?3,updated_at=?4,updated_by=?5 WHERE uuid=?3",
        params![
            if due.is_none() {
                Some(&saved.current_date)
            } else {
                None
            },
            due,
            saved.first_todo_uuid,
            now,
            saved.updated_by
        ],
    )
    .map_err(db_error)?;
    tx.execute("INSERT INTO recurrence_rules(uuid,current_todo_uuid,active,record_json) VALUES(?1,?2,1,?3)",
        params![saved.uuid,saved.current_todo_uuid,serde_json::to_string(&saved).map_err(|_| "INVALID_RECURRENCE_EDIT")?]).map_err(db_error)?;
    tx.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![receipt_key, fingerprint],
    )
    .map_err(db_error)?;
    recurrence_store::snapshot(&tx)?;
    tx.commit().map_err(db_error)?;
    Ok(saved)
}

// Optimistic concurrency prevents a stale editor from stopping a newly advanced rule.
pub fn stop_rule(
    connection: &mut Connection,
    expected: &RecurrenceRule,
    now: i64,
    device_id: &str,
) -> Result<RecurrenceRule, String> {
    validate_rule(expected)?;
    let mut checked = expected.clone();
    if !(0..=MAX_SAFE).contains(&now) {
        return Err("INVALID_RECURRENCE_PROGRESS".into());
    }
    checked.updated_at = now.max(expected.updated_at);
    checked.updated_by = device_id.into();
    validate_rule(&checked)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let mut rule = recurrence_store::snapshot(&tx)?
        .document
        .rules
        .into_iter()
        .find(|r| r.uuid == expected.uuid)
        .ok_or("RECURRENCE_EDIT_CONFLICT")?;
    if rule.deleted_at.is_some() {
        return Ok(rule);
    }
    if &rule != expected {
        return Err("RECURRENCE_EDIT_CONFLICT".into());
    }
    rule.updated_at = stamp(now, rule.updated_at)?;
    rule.updated_by = device_id.into();
    rule.deleted_at = Some(rule.updated_at);
    tx.execute(
        "UPDATE recurrence_rules SET active=0,record_json=?1 WHERE uuid=?2",
        params![
            serde_json::to_string(&rule).map_err(|_| "INVALID_RECURRENCE_EDIT")?,
            rule.uuid
        ],
    )
    .map_err(db_error)?;
    recurrence_store::snapshot(&tx)?;
    tx.commit().map_err(db_error)?;
    Ok(rule)
}

#[cfg(test)]
#[path = "recurrence_editor_tests.rs"]
mod tests;
