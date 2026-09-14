use crate::{
    recurrence_protocol::RecurrenceDocument, recurrence_store, task_checklist_editor::TaskFields,
};
use crate::{task_checklist_protocol as protocol, task_checklist_store as store};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Serialize)]
pub struct ChecklistPanelSnapshot {
    pub todo_uuid: String,
    pub title: String,
    pub note: String,
    pub updated_at: i64,
    pub read_only: bool,
    pub items: protocol::ItemsDocument,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ChecklistProgress {
    pub todo_uuid: String,
    pub total: usize,
    pub completed: usize,
}
#[derive(Debug, Serialize)]
pub struct ChecklistEditorSnapshot {
    pub task: ChecklistPanelSnapshot,
    pub fields: TaskFields,
    pub completed: bool,
    pub repeat_series_uuid: Option<String>,
    pub rules: RecurrenceDocument,
    pub definitions: protocol::DefinitionsDocument,
    pub next_occurrence_date: Option<String>,
}

// Every editable field and conflict baseline belongs to this same SQLite snapshot.
pub fn read_editor(db: &mut Connection, uuid: &str) -> Result<ChecklistEditorSnapshot, String> {
    if !protocol::valid_uuid(uuid) {
        return Err("INVALID_CHECKLIST_SAVE".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let (mut task, fields, completed, series) = tx.query_row(
        "SELECT title,COALESCE(note,''),updated_at,archived_at,due_date,due_at,reminder_at,group_uuid,priority,repeat_rule,completed,repeat_series_uuid FROM todos WHERE uuid=?1 AND deleted_at IS NULL",
        [uuid], |r| Ok((
            ChecklistPanelSnapshot { todo_uuid: uuid.into(), title: r.get(0)?, note: r.get(1)?, updated_at: r.get(2)?,
                read_only: r.get::<_, Option<i64>>(3)?.is_some(), items: Default::default() },
            TaskFields { due_date: r.get(4)?, due_at: r.get(5)?, reminder_at: r.get(6)?, group_uuid: r.get(7)?, priority: r.get(8)?, repeat_rule: r.get(9)? },
            r.get::<_, bool>(10)?, r.get::<_, Option<String>>(11)?
        ))).optional().map_err(|e| e.to_string())?.ok_or("CHECKLIST_PARENT_MISSING")?;
    let checklist = store::read_in_transaction(&tx)?;
    task.items.items = checklist
        .items
        .items
        .into_iter()
        .filter(|i| i.todo_uuid == uuid)
        .collect();
    let rules = recurrence_store::snapshot(&tx)?.document;
    let active: Vec<_> = rules
        .rules
        .iter()
        .filter(|rule| {
            rule.current_todo_uuid == uuid && rule.deleted_at.is_none() && !rule.exhausted
        })
        .collect();
    let next_occurrence_date = if active.len() == 1 {
        crate::recurrence::next_recurrence(&active[0].schedule, &active[0].current_date)?
            .map(|next| next.date)
    } else {
        None
    };
    let result = ChecklistEditorSnapshot {
        task,
        fields,
        completed,
        repeat_series_uuid: series,
        rules,
        definitions: checklist.definitions,
        next_occurrence_date,
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn read(db: &mut Connection, uuid: &str) -> Result<ChecklistPanelSnapshot, String> {
    if !protocol::valid_uuid(uuid) {
        return Err("INVALID_CHECKLIST_SAVE".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let mut panel = tx.query_row("SELECT title,COALESCE(note,''),updated_at,archived_at FROM todos WHERE uuid=?1 AND deleted_at IS NULL", [uuid], |r| {
        Ok(ChecklistPanelSnapshot { todo_uuid: uuid.into(), title:r.get(0)?,note:r.get(1)?,updated_at:r.get(2)?,
            read_only:r.get::<_,Option<i64>>(3)?.is_some(),items:Default::default() })
    }).optional().map_err(|e|e.to_string())?.ok_or("CHECKLIST_PARENT_MISSING")?;
    panel.items.items = store::read_in_transaction(&tx)?
        .items
        .items
        .into_iter()
        .filter(|i| i.todo_uuid == uuid)
        .collect();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(panel)
}
pub fn progress(db: &mut Connection) -> Result<Vec<ChecklistProgress>, String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let parents: HashSet<String> = {
        let mut q = tx
            .prepare("SELECT uuid FROM todos WHERE deleted_at IS NULL")
            .map_err(|e| e.to_string())?;
        let rows = q.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    let mut counts: BTreeMap<String, ChecklistProgress> = BTreeMap::new();
    for item in store::read_in_transaction(&tx)?.items.items {
        if item.deleted_at.is_some() || !parents.contains(&item.todo_uuid) {
            continue;
        }
        let row = counts
            .entry(item.todo_uuid.clone())
            .or_insert(ChecklistProgress {
                todo_uuid: item.todo_uuid,
                total: 0,
                completed: 0,
            });
        row.total += 1;
        row.completed += usize::from(item.completed);
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(counts.into_values().collect())
}
