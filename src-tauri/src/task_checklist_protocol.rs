use crate::recurrence::{validate_recurrence, RecurrenceSchedule};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::{Uuid, Variant};

pub const MAX_CLOCK: i64 = 9_007_199_254_740_991;
const MAX_BYTES: usize = 4 * 1024 * 1024;
fn invalid() -> String {
    "INVALID_CHECKLIST_DOCUMENT".into()
}
pub fn valid_uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok_and(|u| {
        u.to_string() == s
            && (1..=5).contains(&u.get_version_num())
            && u.get_variant() == Variant::RFC4122
    })
}
pub fn valid_text(s: &str, limit: usize, multiline: bool) -> bool {
    s.encode_utf16().count() <= limit
        && s.chars().all(|c| {
            let n = c as u32;
            !(n <= 31 && !(multiline && matches!(n, 9 | 10 | 13))
                || (127..=159).contains(&n)
                || (0x202a..=0x202e).contains(&n)
                || (0x2066..=0x2069).contains(&n)
                || (!multiline && matches!(n, 0x2028 | 0x2029)))
        })
}
pub fn item_uuid(todo: &str, entry: &str) -> Result<String, String> {
    if !valid_uuid(todo) || !valid_uuid(entry) {
        return Err(invalid());
    }
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("eggdone:task-checklist-item:v1:{todo}:{entry}").as_bytes(),
    )
    .to_string())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistItem {
    pub uuid: String,
    pub todo_uuid: String,
    pub source_rule_uuid: Option<String>,
    pub source_entry_uuid: Option<String>,
    pub content: String,
    pub sort_order: i64,
    pub completed: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionEntry {
    pub uuid: String,
    pub content: String,
    pub sort_order: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistDefinition {
    pub rule_uuid: String,
    pub first_todo_uuid: String,
    pub schedule: RecurrenceSchedule,
    pub timezone_id: Option<String>,
    pub applies_from_index: i64,
    pub entries: Vec<DefinitionEntry>,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemsDocument {
    pub format_version: u32,
    pub items: Vec<ChecklistItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionsDocument {
    pub format_version: u32,
    pub definitions: Vec<ChecklistDefinition>,
}
impl Default for ItemsDocument {
    fn default() -> Self {
        Self {
            format_version: 1,
            items: vec![],
        }
    }
}
impl Default for DefinitionsDocument {
    fn default() -> Self {
        Self {
            format_version: 1,
            definitions: vec![],
        }
    }
}
fn stamp(created: i64, updated: i64, by: &str, deleted: Option<i64>) -> bool {
    (0..=MAX_CLOCK).contains(&created)
        && (created..=MAX_CLOCK).contains(&updated)
        && !by.is_empty()
        && by.len() <= 128
        && by
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        && deleted.is_none_or(|d| (created..=updated).contains(&d))
}
fn content(s: &str) -> bool {
    !s.is_empty()
        && s.trim() == s
        && !s.starts_with('\u{feff}')
        && !s.ends_with('\u{feff}')
        && valid_text(s, 200, false)
}
pub fn validate_item(i: &ChecklistItem) -> Result<(), String> {
    if !valid_uuid(&i.uuid)
        || !valid_uuid(&i.todo_uuid)
        || !content(&i.content)
        || !(0..=MAX_CLOCK).contains(&i.sort_order)
        || !stamp(i.created_at, i.updated_at, &i.updated_by, i.deleted_at)
    {
        return Err(invalid());
    }
    match (&i.source_rule_uuid, &i.source_entry_uuid) {
        (None, None) => (),
        (Some(r), Some(e)) if valid_uuid(r) && i.uuid == item_uuid(&i.todo_uuid, e)? => (),
        _ => return Err(invalid()),
    }
    Ok(())
}
pub fn validate_definition(d: &ChecklistDefinition) -> Result<(), String> {
    if !valid_uuid(&d.rule_uuid)
        || !valid_uuid(&d.first_todo_uuid)
        || d.applies_from_index != 2
        || d.entries.len() > 1000
        || !stamp(d.created_at, d.updated_at, &d.updated_by, d.deleted_at)
    {
        return Err(invalid());
    }
    validate_recurrence(&d.schedule).map_err(|_| invalid())?;
    match (&d.timezone_id, d.schedule.local_time_minutes) {
        (None, None) => (),
        (Some(z), Some(_))
            if !z.is_empty()
                && z.len() <= 128
                && !z.starts_with('/')
                && !z.ends_with('/')
                && !z.contains("//")
                && z.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_+-/".contains(&b)) =>
        {
            ()
        }
        _ => return Err(invalid()),
    }
    let mut ids = BTreeMap::new();
    for e in &d.entries {
        if !valid_uuid(&e.uuid)
            || !content(&e.content)
            || !(0..=MAX_CLOCK).contains(&e.sort_order)
            || ids.insert(&e.uuid, ()).is_some()
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn sized<T: Serialize>(doc: &T) -> Result<String, String> {
    let text = serde_json::to_string(doc).map_err(|_| invalid())?;
    if text.len() > MAX_BYTES {
        return Err(invalid());
    }
    Ok(text)
}
pub fn encode_items(doc: &ItemsDocument) -> Result<String, String> {
    if doc.format_version != 1 || doc.items.len() > 10000 {
        return Err(invalid());
    }
    let mut d = doc.clone();
    d.items.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    for (n, i) in d.items.iter().enumerate() {
        validate_item(i)?;
        if n > 0 && d.items[n - 1].uuid == i.uuid {
            return Err(invalid());
        }
    }
    sized(&d)
}
pub fn encode_definitions(doc: &DefinitionsDocument) -> Result<String, String> {
    if doc.format_version != 1
        || doc.definitions.len() > 2000
        || doc
            .definitions
            .iter()
            .map(|d| d.entries.len())
            .sum::<usize>()
            > 20000
    {
        return Err(invalid());
    }
    let mut d = doc.clone();
    d.definitions.sort_by(|a, b| a.rule_uuid.cmp(&b.rule_uuid));
    for i in 0..d.definitions.len() {
        validate_definition(&d.definitions[i])?;
        if i > 0 && d.definitions[i - 1].rule_uuid == d.definitions[i].rule_uuid {
            return Err(invalid());
        }
        d.definitions[i].entries.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    }
    sized(&d)
}
fn normalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(a) => a.iter_mut().for_each(normalize),
        serde_json::Value::Object(o) => o.values_mut().for_each(normalize),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() <= MAX_CLOCK as f64 {
                    *value = serde_json::Value::Number((f as i64).into());
                }
            }
        }
        _ => (),
    }
}
fn raw(source: &str, kind: &str) -> Result<serde_json::Value, String> {
    if source.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut v: serde_json::Value = serde_json::from_str(source).map_err(|_| invalid())?;
    normalize(&mut v);
    for row in v[kind].as_array().ok_or_else(invalid)? {
        let required: &[&str] = if kind == "items" {
            &["source_rule_uuid", "source_entry_uuid", "deleted_at"]
        } else {
            &["timezone_id", "deleted_at"]
        };
        if required.iter().any(|key| row.get(key).is_none()) {
            return Err(invalid());
        }
        if kind == "definitions" {
            for key in [
                "month_day",
                "end_date",
                "max_occurrences",
                "local_time_minutes",
            ] {
                if row["schedule"].get(key).is_none() {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(v)
}
pub fn parse_items(source: &str) -> Result<ItemsDocument, String> {
    let d: ItemsDocument = serde_json::from_value(raw(source, "items")?).map_err(|_| invalid())?;
    encode_items(&d)?;
    Ok(d)
}
pub fn parse_definitions(source: &str) -> Result<DefinitionsDocument, String> {
    let d: DefinitionsDocument =
        serde_json::from_value(raw(source, "definitions")?).map_err(|_| invalid())?;
    encode_definitions(&d)?;
    Ok(d)
}
pub fn same_item_identity(a: &ChecklistItem, b: &ChecklistItem) -> bool {
    a.todo_uuid == b.todo_uuid
        && a.source_rule_uuid == b.source_rule_uuid
        && a.source_entry_uuid == b.source_entry_uuid
        && a.created_at == b.created_at
}
fn item_rank(i: &ChecklistItem) -> (bool, i64, &str, i64, bool, i64, &str) {
    (
        i.deleted_at.is_some(),
        i.updated_at,
        &i.updated_by,
        i.deleted_at.unwrap_or(-1),
        i.completed,
        i.sort_order,
        &i.content,
    )
}
pub fn merge_items(a: &ItemsDocument, b: &ItemsDocument) -> Result<ItemsDocument, String> {
    encode_items(a)?;
    encode_items(b)?;
    let mut rows: BTreeMap<String, ChecklistItem> = BTreeMap::new();
    for i in a.items.iter().chain(&b.items) {
        if let Some(old) = rows.get(&i.uuid) {
            if !same_item_identity(old, i) {
                return Err("ITEM_IDENTITY_CONFLICT".into());
            }
            if item_rank(old) >= item_rank(i) {
                continue;
            }
        }
        rows.insert(i.uuid.clone(), i.clone());
    }
    let d = ItemsDocument {
        format_version: 1,
        items: rows.into_values().collect(),
    };
    encode_items(&d)?;
    Ok(d)
}
pub fn merge_definitions(
    a: &DefinitionsDocument,
    b: &DefinitionsDocument,
) -> Result<DefinitionsDocument, String> {
    let a = parse_definitions(&encode_definitions(a)?)?;
    let b = parse_definitions(&encode_definitions(b)?)?;
    let mut rows: BTreeMap<String, ChecklistDefinition> = BTreeMap::new();
    for i in a.definitions.iter().chain(&b.definitions) {
        if let Some(old) = rows.get(&i.rule_uuid) {
            if old.first_todo_uuid != i.first_todo_uuid
                || old.schedule != i.schedule
                || old.timezone_id != i.timezone_id
                || old.applies_from_index != i.applies_from_index
                || old.entries != i.entries
                || old.created_at != i.created_at
            {
                return Err("DEFINITION_IDENTITY_CONFLICT".into());
            }
            let rank = |d: &ChecklistDefinition| {
                (
                    d.deleted_at.is_some(),
                    d.updated_at,
                    d.updated_by.clone(),
                    d.deleted_at.unwrap_or(-1),
                )
            };
            if rank(old) >= rank(i) {
                continue;
            }
        }
        rows.insert(i.rule_uuid.clone(), i.clone());
    }
    let d = DefinitionsDocument {
        format_version: 1,
        definitions: rows.into_values().collect(),
    };
    encode_definitions(&d)?;
    Ok(d)
}
