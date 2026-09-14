//! P1b storage API. UI, recurrence materialization and transport are wired in later phases.
use crate::task_checklist_protocol::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq)]
pub struct DomainState {
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub generation: i64,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub items: ItemsDocument,
    pub definitions: DefinitionsDocument,
    pub item_state: DomainState,
    pub definition_state: DomainState,
}
fn error(e: rusqlite::Error) -> String {
    format!("CHECKLIST_DATABASE: {e}")
}
fn state(db: &Connection, domain: &str) -> Result<DomainState, String> {
    let s = db.query_row("SELECT revision,synced_revision,etag,generation FROM task_checklist_sync_state WHERE domain=?1", [domain],
        |r| Ok(DomainState { revision:r.get(0)?, synced_revision:r.get(1)?, etag:r.get(2)?, generation:r.get(3)? })).map_err(error)?;
    if !(0..=MAX_CLOCK).contains(&s.revision)
        || !(0..=s.revision).contains(&s.synced_revision)
        || !(0..=MAX_CLOCK).contains(&s.generation)
    {
        return Err("CHECKLIST_STATE_INVALID".into());
    }
    Ok(s)
}
pub fn read_in_transaction(tx: &Transaction<'_>) -> Result<Snapshot, String> {
    let item_state = state(tx, "items")?;
    let definition_state = state(tx, "definitions")?;
    let mut items = ItemsDocument::default();
    let mut query = tx
        .prepare("SELECT uuid,todo_uuid,active,record_json FROM task_checklist_items ORDER BY uuid")
        .map_err(error)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(error)?;
    for row in rows {
        let (id, todo, active, json) = row.map_err(error)?;
        let doc = parse_items(&format!("{{\"format_version\":1,\"items\":[{json}]}}"))?;
        let i = &doc.items[0];
        if i.uuid != id || i.todo_uuid != todo || active != i64::from(i.deleted_at.is_none()) {
            return Err("CHECKLIST_INDEX_INVALID".into());
        }
        items.items.extend(doc.items);
    }
    let mut definitions = DefinitionsDocument::default();
    let mut query=tx.prepare("SELECT rule_uuid,active,record_json FROM task_checklist_definitions ORDER BY rule_uuid").map_err(error)?;
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
        let (id, active, json) = row.map_err(error)?;
        let doc = parse_definitions(&format!(
            "{{\"format_version\":1,\"definitions\":[{json}]}}"
        ))?;
        let d = &doc.definitions[0];
        if d.rule_uuid != id || active != i64::from(d.deleted_at.is_none()) {
            return Err("CHECKLIST_INDEX_INVALID".into());
        }
        definitions.definitions.extend(doc.definitions);
    }
    encode_items(&items)?;
    encode_definitions(&definitions)?;
    Ok(Snapshot {
        items,
        definitions,
        item_state,
        definition_state,
    })
}
pub fn snapshot(db: &mut Connection) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
/// The caller must roll back the transaction if validation or any write fails.
pub fn merge_in_transaction(
    tx: &Transaction<'_>,
    items: &ItemsDocument,
    definitions: &DefinitionsDocument,
) -> Result<(), String> {
    let current = read_in_transaction(tx)?;
    let items = merge_items(&current.items, items)?;
    let definitions = parse_definitions(&encode_definitions(&merge_definitions(
        &current.definitions,
        definitions,
    )?)?)?;
    for i in items.items {
        let json = serde_json::to_string(&i).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO task_checklist_items(uuid,todo_uuid,active,record_json) VALUES(?1,?2,?3,?4)
          ON CONFLICT(uuid) DO UPDATE SET active=excluded.active,record_json=excluded.record_json WHERE record_json<>excluded.record_json",
          params![i.uuid,i.todo_uuid,i.deleted_at.is_none(),json]).map_err(error)?;
    }
    for d in definitions.definitions {
        let json = serde_json::to_string(&d).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO task_checklist_definitions(rule_uuid,active,record_json) VALUES(?1,?2,?3)
          ON CONFLICT(rule_uuid) DO UPDATE SET active=excluded.active,record_json=excluded.record_json WHERE record_json<>excluded.record_json",
          params![d.rule_uuid,d.deleted_at.is_none(),json]).map_err(error)?;
    }
    Ok(())
}
pub fn merge(
    db: &mut Connection,
    items: &ItemsDocument,
    definitions: &DefinitionsDocument,
) -> Result<Snapshot, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    merge_in_transaction(&tx, items, definitions)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistEdit {
    pub uuid: String,
    pub content: String,
    pub sort_order: i64,
    pub completed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistSave {
    pub operation_uuid: String,
    pub todo_uuid: String,
    pub expected_updated_at: i64,
    pub expected_items: ItemsDocument,
    pub title: String,
    pub note: String,
    pub items: Vec<ChecklistEdit>,
}
pub fn save(
    db: &mut Connection,
    request: &ChecklistSave,
    now: i64,
    by: &str,
) -> Result<i64, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let result = save_in_transaction(&tx, request, now, by)?;
    tx.commit().map_err(error)?;
    Ok(result)
}
/// Existing-parent edit. Only title/note and children change; date/reminder/recurrence are preserved.
pub fn save_in_transaction(
    tx: &Transaction<'_>,
    r: &ChecklistSave,
    now: i64,
    by: &str,
) -> Result<i64, String> {
    if !valid_uuid(&r.operation_uuid)
        || !valid_uuid(&r.todo_uuid)
        || !valid_uuid(by)
        || !(0..=MAX_CLOCK).contains(&now)
        || !(0..=MAX_CLOCK).contains(&r.expected_updated_at)
        || r.title.trim().is_empty()
        || !valid_text(&r.title, 100, false)
        || !valid_text(&r.note, 1000, true)
        || r.items.len() > 10000
    {
        return Err("INVALID_CHECKLIST_SAVE".into());
    }
    let mut canonical = r.clone();
    canonical.expected_items = parse_items(&encode_items(&r.expected_items)?)?;
    canonical
        .expected_items
        .items
        .sort_by(|a, b| a.uuid.cmp(&b.uuid));
    canonical.items.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    if canonical
        .expected_items
        .items
        .iter()
        .any(|i| i.todo_uuid != r.todo_uuid)
    {
        return Err("INVALID_CHECKLIST_SAVE".into());
    }
    let payload = serde_json::to_string(&(&canonical, by)).map_err(|e| e.to_string())?;
    let receipt:Option<(String,i64)>=tx.query_row("SELECT payload,result_updated_at FROM task_checklist_operations WHERE operation_uuid=?1",[&r.operation_uuid],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(error)?;
    if let Some((old, result)) = receipt {
        return if old == payload {
            Ok(result)
        } else {
            Err("CHECKLIST_OPERATION_REUSED".into())
        };
    }
    let (title,note,updated,deleted,archived):(String,String,i64,Option<i64>,Option<i64>)=tx.query_row(
      "SELECT title,COALESCE(note,''),updated_at,deleted_at,archived_at FROM todos WHERE uuid=?1",[&r.todo_uuid],
      |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).map_err(error)?;
    if deleted.is_some() || archived.is_some() {
        return Err("CHECKLIST_PARENT_READ_ONLY".into());
    }
    let all = read_in_transaction(tx)?.items;
    let current = ItemsDocument {
        format_version: 1,
        items: all
            .items
            .iter()
            .filter(|i| i.todo_uuid == r.todo_uuid)
            .cloned()
            .collect(),
    };
    if updated != r.expected_updated_at
        || encode_items(&current)? != encode_items(&r.expected_items)?
    {
        return Err("CHECKLIST_STALE_DRAFT".into());
    }
    let clock = now.max(updated + 1).max(
        current
            .items
            .iter()
            .map(|i| i.updated_at + 1)
            .max()
            .unwrap_or(0),
    );
    if clock > MAX_CLOCK {
        return Err("CHECKLIST_CLOCK_EXHAUSTED".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut patch = ItemsDocument::default();
    let mut added = false;
    for edit in &r.items {
        if !ids.insert(edit.uuid.clone()) {
            return Err("INVALID_CHECKLIST_SAVE".into());
        }
        let old = all.items.iter().find(|i| i.uuid == edit.uuid);
        let mut item = match old {
            Some(i) if i.todo_uuid == r.todo_uuid && i.deleted_at.is_none() => i.clone(),
            Some(_) => return Err("CHECKLIST_ID_REUSED".into()),
            None => {
                added = true;
                ChecklistItem {
                    uuid: edit.uuid.clone(),
                    todo_uuid: r.todo_uuid.clone(),
                    source_rule_uuid: None,
                    source_entry_uuid: None,
                    content: edit.content.clone(),
                    sort_order: edit.sort_order,
                    completed: edit.completed,
                    created_at: clock,
                    updated_at: clock,
                    updated_by: by.into(),
                    deleted_at: None,
                }
            }
        };
        if item.content != edit.content
            || item.sort_order != edit.sort_order
            || item.completed != edit.completed
        {
            item.content = edit.content.clone();
            item.sort_order = edit.sort_order;
            item.completed = edit.completed;
            item.updated_at = clock;
            item.updated_by = by.into();
        }
        validate_item(&item)?;
        if old != Some(&item) {
            patch.items.push(item);
        }
    }
    if added && r.items.len() > 20 {
        return Err("CHECKLIST_LOCAL_LIMIT".into());
    }
    for mut i in current.items {
        if i.deleted_at.is_none() && !ids.contains(&i.uuid) {
            i.deleted_at = Some(clock);
            i.updated_at = clock;
            i.updated_by = by.into();
            patch.items.push(i);
        }
    }
    let changed = title != r.title || note != r.note || !patch.items.is_empty();
    let result = if changed { clock } else { updated };
    if changed {
        tx.execute(
            "UPDATE todos SET title=?1,note=?2,updated_at=?3,updated_by=?4 WHERE uuid=?5",
            params![r.title, r.note, result, by, r.todo_uuid],
        )
        .map_err(error)?;
        merge_in_transaction(tx, &patch, &DefinitionsDocument::default())?;
    }
    tx.execute("INSERT INTO task_checklist_operations(operation_uuid,payload,result_updated_at) VALUES(?1,?2,?3)",params![r.operation_uuid,payload,result]).map_err(error)?;
    Ok(result)
}
pub fn visible_items(db: &mut Connection, todo: &str) -> Result<Vec<ChecklistItem>, String> {
    let tx = db.transaction().map_err(error)?;
    let visible: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM todos WHERE uuid=?1 AND deleted_at IS NULL)",
            [todo],
            |r| r.get(0),
        )
        .map_err(error)?;
    let mut items = if visible {
        read_in_transaction(&tx)?
            .items
            .items
            .into_iter()
            .filter(|i| i.todo_uuid == todo && i.deleted_at.is_none())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.uuid.cmp(&b.uuid)));
    tx.commit().map_err(error)?;
    Ok(items)
}
