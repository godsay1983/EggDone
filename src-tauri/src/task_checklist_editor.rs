//! Complete draft transaction; system reminders are reconciled by the caller only after commit.
use crate::recurrence::RecurrenceSchedule;
use crate::recurrence_protocol::{self as rules, RecurrenceDocument, RecurrenceRule};
use crate::task_checklist_protocol::*;
use crate::task_checklist_store::{self as checklist, ChecklistSave};
use crate::{recurrence_store, recurrence_time};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFields {
    pub due_date: Option<String>,
    pub due_at: Option<i64>,
    pub reminder_at: Option<i64>,
    pub group_uuid: Option<String>,
    pub priority: i64,
    pub repeat_rule: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSeed {
    pub uuid: String,
    pub schedule: RecurrenceSchedule,
    pub timezone_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRequest {
    pub task: ChecklistSave,
    pub fields: TaskFields,
    pub expected_rules: RecurrenceDocument,
    pub mode: String,
    pub replaces_uuid: Option<String>,
    pub replacement: Option<RuleSeed>,
    pub future_entries: Vec<DefinitionEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorResult {
    pub updated_at: i64,
    pub rule_uuid: Option<String>,
    pub reminder_changed: bool,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    fingerprint: String,
    result: EditorResult,
}
fn invalid() -> String {
    "INVALID_CHECKLIST_EDITOR".into()
}
fn db_error(e: rusqlite::Error) -> String {
    format!("CHECKLIST_EDITOR_DATABASE: {e}")
}
fn canonical(r: &EditorRequest) -> Result<EditorRequest, String> {
    let mut r = r.clone();
    r.expected_rules = rules::parse_document(&rules::encode_document(&r.expected_rules)?)?;
    r.task.expected_items = parse_items(&encode_items(&r.task.expected_items)?)?;
    r.task
        .expected_items
        .items
        .sort_by(|a, b| a.uuid.cmp(&b.uuid));
    r.task.items.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    r.future_entries.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(r)
}
fn validate_fields(f: &TaskFields) -> Result<(), String> {
    if ![0, 1].contains(&f.priority)
        || f.group_uuid.as_deref().is_some_and(|s| !valid_uuid(s))
        || [f.due_at, f.reminder_at]
            .iter()
            .flatten()
            .any(|n| !(0..=MAX_CLOCK).contains(n))
        || (f.due_date.is_some() && f.due_at.is_some())
        || f.repeat_rule
            .as_deref()
            .is_some_and(|s| !["daily", "weekly", "monthly", "weekdays"].contains(&s))
        || (f.repeat_rule.is_some() && f.due_date.is_none() && f.due_at.is_none())
    {
        return Err(invalid());
    }
    if let Some(d) = &f.due_date {
        recurrence_time::recurrence_due_at(d, None, None)?;
    }
    Ok(())
}
pub fn save(
    db: &mut Connection,
    request: &EditorRequest,
    now: i64,
    by: &str,
) -> Result<EditorResult, String> {
    let r = canonical(request)?;
    validate_fields(&r.fields)?;
    if !valid_uuid(&r.task.operation_uuid)
        || !valid_uuid(by)
        || !(0..MAX_CLOCK).contains(&now)
        || !["keep", "replace", "stop"].contains(&r.mode.as_str())
    {
        return Err(invalid());
    }
    if r.mode != "replace" && (r.replacement.is_some() || !r.future_entries.is_empty())
        || r.mode == "keep" && r.replaces_uuid.is_some()
    {
        return Err(invalid());
    }
    let fingerprint = serde_json::to_string(&(&r, by)).map_err(|_| invalid())?;
    let key = format!("checklist.editor.v1:{}", r.task.operation_uuid);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let receipt: Option<String> = tx
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if let Some(text) = receipt {
        let receipt: Receipt = serde_json::from_str(&text).map_err(|_| invalid())?;
        if receipt.fingerprint != fingerprint {
            return Err("CHECKLIST_OPERATION_REUSED".into());
        }
        tx.commit().map_err(db_error)?;
        return Ok(receipt.result);
    }
    let (before,series,completed):(TaskFields,Option<String>,bool)=tx.query_row(
      "SELECT due_date,due_at,reminder_at,group_uuid,priority,repeat_rule,repeat_series_uuid,completed FROM todos WHERE uuid=?1",[&r.task.todo_uuid],
      |row|Ok((TaskFields{due_date:row.get(0)?,due_at:row.get(1)?,reminder_at:row.get(2)?,group_uuid:row.get(3)?,priority:row.get(4)?,repeat_rule:row.get(5)?},row.get(6)?,row.get(7)?))).map_err(db_error)?;
    if r.fields.reminder_at != before.reminder_at && r.fields.reminder_at.is_some_and(|n| n <= now)
    {
        return Err("CHECKLIST_REMINDER_PAST".into());
    }
    if let Some(g) = &r.fields.group_uuid {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid=?1 AND deleted_at IS NULL)",
                [g],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if !exists {
            return Err("CHECKLIST_GROUP_MISSING".into());
        }
    }
    let current = recurrence_store::snapshot(&tx)?.document;
    if rules::encode_document(&current)? != rules::encode_document(&r.expected_rules)? {
        return Err("CHECKLIST_RULE_STALE".into());
    }
    let active: Vec<_> = current
        .rules
        .iter()
        .filter(|rule| {
            rule.deleted_at.is_none()
                && !rule.exhausted
                && rule.current_todo_uuid == r.task.todo_uuid
        })
        .collect();
    let mut old: Option<RecurrenceRule> = None;
    if let Some(id) = &r.replaces_uuid {
        let rule = current
            .rules
            .iter()
            .find(|rule| &rule.uuid == id)
            .ok_or_else(invalid)?;
        if rule.current_todo_uuid != r.task.todo_uuid
            || rule.deleted_at.is_some()
            || rule.exhausted
            || series.as_ref().is_some_and(|s| s != &rule.first_todo_uuid)
            || (series.is_none() && rule.generated_count != 1)
            || before.repeat_rule.is_some()
        {
            return Err("CHECKLIST_RULE_CONFLICT".into());
        }
        old = Some(rule.clone());
    }
    if r.mode == "keep" {
        if (!active.is_empty() || (series.is_some() && before.repeat_rule.is_none()))
            && r.fields.repeat_rule != before.repeat_rule
        {
            return Err("CHECKLIST_RULE_CONFLICT".into());
        }
    } else {
        if completed
            || r.fields.repeat_rule.is_some()
            || active
                .iter()
                .any(|rule| Some(&rule.uuid) != r.replaces_uuid.as_ref())
        {
            return Err("CHECKLIST_RULE_CONFLICT".into());
        }
        if old.is_none() && series.is_some() && before.repeat_rule.is_none() {
            return Err("CHECKLIST_RULE_CONFLICT".into());
        }
        if r.mode == "stop" && old.is_none() && before.repeat_rule.is_none() {
            return Err(invalid());
        }
    }
    let mut clock = now.max(
        r.task
            .expected_updated_at
            .checked_add(1)
            .ok_or_else(invalid)?,
    );
    if let Some(old) = &old {
        clock = clock.max(old.updated_at + 1);
    }
    if clock > MAX_CLOCK {
        return Err("CHECKLIST_CLOCK_EXHAUSTED".into());
    }
    let mut body = r.task.clone();
    body.operation_uuid = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_DNS,
        format!("eggdone:checklist-editor-body:v1:{}", r.task.operation_uuid).as_bytes(),
    )
    .to_string();
    let orphan: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM task_checklist_operations WHERE operation_uuid=?1)",
            [&body.operation_uuid],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if orphan {
        return Err("CHECKLIST_EDITOR_RECEIPT_CONFLICT".into());
    }
    let body_clock = checklist::save_in_transaction(&tx, &body, clock, by)?;
    let fields_changed = r.fields != before || r.mode != "keep";
    let updated = if fields_changed {
        body_clock.max(clock)
    } else {
        body_clock
    };
    let mut updates = RecurrenceDocument {
        format_version: 1,
        rules: vec![],
    };
    let mut definitions = DefinitionsDocument::default();
    let mut rule_uuid = active.first().map(|rule| rule.uuid.clone());
    let mut next_series = series.clone();
    if r.mode != "keep" {
        if let Some(mut old) = old {
            old.deleted_at = Some(updated);
            old.updated_at = updated;
            old.updated_by = by.into();
            updates.rules.push(old);
        }
        next_series = None;
        rule_uuid = None;
    } else if r.fields.repeat_rule != before.repeat_rule {
        next_series = r
            .fields
            .repeat_rule
            .as_ref()
            .map(|_| series.clone().unwrap_or_else(|| r.task.todo_uuid.clone()));
    }
    if r.mode == "replace" {
        let seed = r.replacement.as_ref().ok_or_else(invalid)?;
        if !valid_uuid(&seed.uuid)
            || current.rules.iter().any(|rule| rule.uuid == seed.uuid)
            || checklist::read_in_transaction(&tx)?
                .definitions
                .definitions
                .iter()
                .any(|d| d.rule_uuid == seed.uuid)
            || r.future_entries.len() > 20
        {
            return Err(invalid());
        }
        let due = recurrence_time::recurrence_due_at(
            &seed.schedule.anchor_date,
            seed.schedule.local_time_minutes,
            seed.timezone_id.as_deref(),
        )?;
        if r.fields.due_at != due
            || r.fields.due_date
                != if due.is_none() {
                    Some(seed.schedule.anchor_date.clone())
                } else {
                    None
                }
        {
            return Err("CHECKLIST_SCHEDULE_MISMATCH".into());
        }
        let rule = RecurrenceRule {
            uuid: seed.uuid.clone(),
            first_todo_uuid: r.task.todo_uuid.clone(),
            schedule: seed.schedule.clone(),
            timezone_id: seed.timezone_id.clone(),
            current_todo_uuid: r.task.todo_uuid.clone(),
            current_date: seed.schedule.anchor_date.clone(),
            generated_count: 1,
            exhausted: false,
            updated_at: updated,
            updated_by: by.into(),
            deleted_at: None,
        };
        rules::validate_rule(&rule)?;
        let definition = ChecklistDefinition {
            rule_uuid: seed.uuid.clone(),
            first_todo_uuid: r.task.todo_uuid.clone(),
            schedule: seed.schedule.clone(),
            timezone_id: seed.timezone_id.clone(),
            applies_from_index: 2,
            entries: r.future_entries.clone(),
            created_at: updated,
            updated_at: updated,
            updated_by: by.into(),
            deleted_at: None,
        };
        validate_definition(&definition)?;
        updates.rules.push(rule);
        definitions.definitions.push(definition);
        next_series = Some(r.task.todo_uuid.clone());
        rule_uuid = Some(seed.uuid.clone());
    }
    if !updates.rules.is_empty() {
        recurrence_store::merge_in_transaction(&tx, &updates)?;
    }
    if !definitions.definitions.is_empty() {
        checklist::merge_in_transaction(&tx, &ItemsDocument::default(), &definitions)?;
    }
    if fields_changed {
        let schedule_changed = r.fields.due_date != before.due_date
            || r.fields.due_at != before.due_at
            || r.fields.repeat_rule != before.repeat_rule
            || r.mode != "keep";
        tx.execute("UPDATE todos SET due_date=?1,due_at=?2,reminder_at=?3,group_uuid=?4,priority=?5,repeat_rule=?6,repeat_series_uuid=?7,
          repeat_next_due_date=CASE WHEN ?8 THEN NULL ELSE repeat_next_due_date END,updated_at=?9,updated_by=?10 WHERE uuid=?11",
          params![r.fields.due_date,r.fields.due_at,r.fields.reminder_at,r.fields.group_uuid,r.fields.priority,r.fields.repeat_rule,next_series,schedule_changed,updated,by,r.task.todo_uuid]).map_err(db_error)?;
    }
    let result = EditorResult {
        updated_at: updated,
        rule_uuid,
        reminder_changed: r.fields.reminder_at != before.reminder_at,
    };
    let receipt = Receipt {
        fingerprint,
        result: result.clone(),
    };
    tx.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![key, serde_json::to_string(&receipt).map_err(|_| invalid())?],
    )
    .map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}
