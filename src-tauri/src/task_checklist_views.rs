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
