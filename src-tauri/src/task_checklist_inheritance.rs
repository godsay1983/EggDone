//! Insert-only inheritance. No wall-clock timestamps, parent creation or reminder side effects.
use crate::{
    recurrence::{
        recurrence_occurrence_key, recurrence_on_date, recurrence_todo_uuid, RecurrenceOccurrence,
    },
    recurrence_protocol::RecurrenceRule,
    recurrence_store,
    recurrence_time::recurrence_due_at,
    task_checklist_protocol::*,
    task_checklist_store,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    rule_uuid: String,
    occurrence_key: String,
    planned_date: String,
    planned_due_at: Option<i64>,
}
struct Parent {
    uuid: String,
    due_date: Option<String>,
    due_at: Option<i64>,
}
fn db(e: rusqlite::Error) -> String {
    format!("CHECKLIST_INHERITANCE_DATABASE: {e}")
}

fn occurrence(
    tx: &Transaction<'_>,
    rule: &RecurrenceRule,
    parent: &Parent,
) -> Result<Option<RecurrenceOccurrence>, String> {
    let receipt: Option<String> = tx
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("recurrence.instance.v1:{}", parent.uuid)],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    if let Some(raw) = receipt {
        let r: Receipt = serde_json::from_str(&raw).map_err(|_| "CHECKLIST_RECEIPT_INVALID")?;
        // A task can be the first instance of a fork, but still own its original receipt.
        if r.rule_uuid != rule.uuid {
            return Ok(None);
        }
        let o = recurrence_on_date(&rule.schedule, &r.planned_date)?;
        if o.index < 2
            || recurrence_occurrence_key(&rule.uuid, &rule.schedule, &o)? != r.occurrence_key
            || recurrence_todo_uuid(&rule.uuid, &rule.schedule, &o)? != parent.uuid
            || recurrence_due_at(
                &o.date,
                rule.schedule.local_time_minutes,
                rule.timezone_id.as_deref(),
            )? != r.planned_due_at
        {
            return Err("CHECKLIST_RECEIPT_INVALID".into());
        }
        return Ok(Some(o));
    }
    let mut dates = Vec::new();
    if rule.current_todo_uuid == parent.uuid {
        dates.push((rule.current_date.clone(), true));
    }
    if let Some(date) = &parent.due_date {
        dates.push((date.clone(), false));
    }
    if let Some(stamp) = parent
        .due_at
        .filter(|_| rule.schedule.local_time_minutes.is_some())
    {
        if let Ok(utc) =
            time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(stamp) * 1_000_000)
        {
            // Candidate dates only; exact schedule/UUID and IANA instant checks below are authoritative.
            for delta in -2..=2 {
                if let Some(date) = utc.date().checked_add(time::Duration::days(delta)) {
                    dates.push((date.to_string(), false));
                }
            }
        }
    }
    for (date, trusted_progress) in dates {
        let Ok(o) = recurrence_on_date(&rule.schedule, &date) else {
            continue;
        };
        if o.index < 2 || recurrence_todo_uuid(&rule.uuid, &rule.schedule, &o)? != parent.uuid {
            continue;
        }
        let due = recurrence_due_at(
            &o.date,
            rule.schedule.local_time_minutes,
            rule.timezone_id.as_deref(),
        )?;
        if trusted_progress
            || (due.is_none() && parent.due_date.as_deref() == Some(&o.date))
            || due == parent.due_at && due.is_some()
        {
            return Ok(Some(o));
        }
    }
    // Remote historical tasks edited away from their original date need a receipt, not a guessed identity.
    Ok(None)
}

/// Caller owns rollback. `only_todo` restricts completion work to its generated instance.
pub fn repair_in_transaction(
    tx: &Transaction<'_>,
    only_todo: Option<&str>,
) -> Result<usize, String> {
    let snapshot = task_checklist_store::read_in_transaction(tx)?;
    if !snapshot
        .definitions
        .definitions
        .iter()
        .any(|d| d.deleted_at.is_none())
    {
        return Ok(0);
    }
    let rules = recurrence_store::snapshot(tx)?.document;
    let mut patch = ItemsDocument::default();
    let existing: std::collections::HashMap<_, _> = snapshot
        .items
        .items
        .iter()
        .map(|i| (i.uuid.as_str(), i))
        .collect();
    for definition in snapshot
        .definitions
        .definitions
        .iter()
        .filter(|d| d.deleted_at.is_none())
    {
        let Some(rule) = rules.rules.iter().find(|r| r.uuid == definition.rule_uuid) else {
            continue;
        };
        if definition.first_todo_uuid != rule.first_todo_uuid
            || definition.schedule != rule.schedule
            || definition.timezone_id != rule.timezone_id
        {
            return Err("CHECKLIST_DEFINITION_RULE_MISMATCH".into());
        }
        let mut query = tx.prepare("SELECT uuid,due_date,due_at FROM todos WHERE repeat_series_uuid=?1 AND repeat_rule IS NULL AND deleted_at IS NULL AND archived_at IS NULL AND (?2 IS NULL OR uuid=?2)").map_err(db)?;
        let parents = query
            .query_map(rusqlite::params![rule.first_todo_uuid, only_todo], |r| {
                Ok(Parent {
                    uuid: r.get(0)?,
                    due_date: r.get(1)?,
                    due_at: r.get(2)?,
                })
            })
            .map_err(db)?;
        for parent in parents {
            let parent = parent.map_err(db)?;
            if parent.uuid == rule.first_todo_uuid {
                continue;
            }
            let Some(o) = occurrence(tx, rule, &parent)? else {
                continue;
            };
            if i64::from(o.index) < definition.applies_from_index {
                continue;
            }
            for entry in &definition.entries {
                let seed = ChecklistItem {
                    uuid: item_uuid(&parent.uuid, &entry.uuid)?,
                    todo_uuid: parent.uuid.clone(),
                    source_rule_uuid: Some(rule.uuid.clone()),
                    source_entry_uuid: Some(entry.uuid.clone()),
                    content: entry.content.clone(),
                    sort_order: entry.sort_order,
                    completed: false,
                    created_at: definition.created_at,
                    updated_at: definition.created_at,
                    updated_by: "checklist-seed-v1".into(),
                    deleted_at: None,
                };
                if let Some(old) = existing.get(seed.uuid.as_str()) {
                    if old.todo_uuid != seed.todo_uuid
                        || old.source_rule_uuid != seed.source_rule_uuid
                        || old.source_entry_uuid != seed.source_entry_uuid
                        || old.created_at != seed.created_at
                    {
                        return Err("ITEM_IDENTITY_CONFLICT".into());
                    }
                } else {
                    if existing.len() + patch.items.len() >= 10000 {
                        return Err("CHECKLIST_INHERITANCE_LIMIT".into());
                    }
                    patch.items.push(seed);
                }
            }
        }
    }
    let count = patch.items.len();
    if count > 0 {
        task_checklist_store::merge_in_transaction(tx, &patch, &DefinitionsDocument::default())?;
    }
    Ok(count)
}

pub fn repair(connection: &mut Connection) -> Result<usize, String> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    let count = repair_in_transaction(&tx, None)?;
    tx.commit().map_err(db)?;
    Ok(count)
}
