//! Local dependency preparation, not evidence that a Todo has been uploaded.
use crate::recurrence_protocol::{encode_document, RecurrenceDocument};
use crate::recurrence_store;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkTodo {
    pub uuid: String,
    pub repeat_rule: Option<String>,
    pub repeat_series_uuid: Option<String>,
    pub completed: bool,
    pub deleted_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoLink {
    pub rule_uuid: String,
    pub todo_uuid: String,
    pub state: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPlan {
    pub links: Vec<TodoLink>,
    pub can_prepare: bool,
}

#[derive(Debug)]
pub struct LinkPreparation {
    pub plan: LinkPlan,
    pub bound_todos: Vec<String>,
    pub rule_revision: i64,
}

pub fn inspect_links(
    document: &RecurrenceDocument,
    todos: &[LinkTodo],
) -> Result<LinkPlan, String> {
    encode_document(document)?;
    let mut tasks = HashMap::new();
    for todo in todos {
        if !(0..=MAX_SAFE).contains(&todo.updated_at)
            || todo
                .deleted_at
                .is_some_and(|n| !(0..=todo.updated_at).contains(&n))
            || todo
                .archived_at
                .is_some_and(|n| !(0..=todo.updated_at).contains(&n))
            || tasks.insert(todo.uuid.as_str(), todo).is_some()
        {
            return Err("INVALID_RECURRENCE_LINKS".into());
        }
    }
    let mut owners = HashMap::new();
    let mut collisions = HashSet::new();
    for rule in document
        .rules
        .iter()
        .filter(|r| r.deleted_at.is_none() && !r.exhausted)
    {
        for id in [&rule.first_todo_uuid, &rule.current_todo_uuid] {
            if let Some(previous) = owners.insert(id, &rule.uuid) {
                if previous != &rule.uuid {
                    collisions.insert(previous);
                    collisions.insert(&rule.uuid);
                }
            }
        }
    }
    let mut links = Vec::new();
    for rule in document
        .rules
        .iter()
        .filter(|r| r.deleted_at.is_none() && !r.exhausted)
    {
        let state = if collisions.contains(&rule.uuid) {
            "conflict"
        } else if let Some(todo) = tasks.get(rule.current_todo_uuid.as_str()) {
            if todo.repeat_rule.is_some()
                || todo
                    .repeat_series_uuid
                    .as_deref()
                    .is_some_and(|s| s != rule.first_todo_uuid)
                || (rule.generated_count > 1 && todo.repeat_series_uuid.is_none())
            {
                "conflict"
            } else if todo.archived_at.is_some() && !todo.completed && todo.deleted_at.is_none() {
                "archived"
            } else if todo.repeat_series_uuid.is_none() {
                "bind"
            } else if todo.completed || todo.deleted_at.is_some() {
                "reconcile"
            } else {
                "ready"
            }
        } else {
            "missing"
        };
        links.push(TodoLink {
            rule_uuid: rule.uuid.clone(),
            todo_uuid: rule.current_todo_uuid.clone(),
            state: state.into(),
        });
    }
    links.sort_by(|a, b| a.rule_uuid.cmp(&b.rule_uuid));
    let can_prepare = links
        .iter()
        .all(|l| matches!(l.state.as_str(), "bind" | "ready" | "reconcile"));
    Ok(LinkPlan { links, can_prepare })
}

fn db_error(error: rusqlite::Error) -> String {
    format!("RECURRENCE_DATABASE: {error}")
}

fn read_todos(
    connection: &Connection,
    document: &RecurrenceDocument,
) -> Result<Vec<LinkTodo>, String> {
    let mut todos = Vec::new();
    for rule in document
        .rules
        .iter()
        .filter(|r| r.deleted_at.is_none() && !r.exhausted)
    {
        let row = connection.query_row("SELECT uuid,repeat_rule,repeat_series_uuid,completed,deleted_at,archived_at,updated_at FROM todos WHERE uuid=?1",
            [&rule.current_todo_uuid], |r| Ok(LinkTodo { uuid: r.get(0)?, repeat_rule: r.get(1)?, repeat_series_uuid: r.get(2)?,
                completed: r.get(3)?, deleted_at: r.get(4)?, archived_at: r.get(5)?, updated_at: r.get(6)? })).optional().map_err(db_error)?;
        if let Some(todo) = row {
            todos.push(todo);
        }
    }
    Ok(todos)
}

pub fn prepare_links(
    connection: &mut Connection,
    now: i64,
    device_id: &str,
) -> Result<LinkPreparation, String> {
    if !(0..MAX_SAFE).contains(&now)
        || device_id.is_empty()
        || device_id.len() > 128
        || !device_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err("INVALID_RECURRENCE_LINKS".into());
    }
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    let snapshot = recurrence_store::snapshot(&tx)?;
    let todos = read_todos(&tx, &snapshot.document)?;
    let plan = inspect_links(&snapshot.document, &todos)?;
    let mut bound_todos = Vec::new();
    if plan.can_prepare {
        for link in plan.links.iter().filter(|l| l.state == "bind") {
            let rule = snapshot
                .document
                .rules
                .iter()
                .find(|r| r.uuid == link.rule_uuid)
                .ok_or("INVALID_RECURRENCE_LINKS")?;
            let todo = todos
                .iter()
                .find(|t| t.uuid == link.todo_uuid)
                .ok_or("INVALID_RECURRENCE_LINKS")?;
            let updated_at = now.max(todo.updated_at + 1).max(rule.updated_at + 1);
            if updated_at > MAX_SAFE {
                return Err("INVALID_RECURRENCE_LINKS".into());
            }
            let changed = tx.execute("UPDATE todos SET repeat_series_uuid=?1,updated_at=?2,updated_by=?3 WHERE uuid=?4 AND updated_at=?5 AND repeat_rule IS NULL AND repeat_series_uuid IS NULL",
                params![rule.first_todo_uuid, updated_at, device_id, todo.uuid, todo.updated_at]).map_err(db_error)?;
            if changed != 1 {
                return Err("RECURRENCE_LINK_CONFLICT".into());
            }
            bound_todos.push(todo.uuid.clone());
        }
    }
    let result = LinkPreparation {
        plan: inspect_links(&snapshot.document, &read_todos(&tx, &snapshot.document)?)?,
        bound_todos,
        rule_revision: snapshot.revision,
    };
    tx.commit().map_err(db_error)?;
    Ok(result)
}

#[cfg(test)]
#[path = "recurrence_links_tests.rs"]
mod tests;
