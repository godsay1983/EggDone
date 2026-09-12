use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::{Uuid, Variant};

pub const MAX_LINKS: usize = 10000;
pub const MAX_DOCUMENT_UNITS: usize = 4_194_304;
pub const MAX_LOCAL_LINKS_PER_TODO: usize = 20;
pub const MAX_CLOCK: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskNoteLink {
    pub uuid: String,
    pub todo_uuid: String,
    pub note_uuid: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkDocument {
    pub format_version: u32,
    pub links: Vec<TaskNoteLink>,
}

fn invalid() -> String {
    "INVALID_TASK_NOTE_LINK_DOCUMENT".into()
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| {
        id.to_string() == value
            && (1..=5).contains(&id.get_version_num())
            && id.get_variant() == Variant::RFC4122
    })
}

pub fn link_uuid(todo: &str, note: &str) -> Result<String, String> {
    let todo = todo.to_ascii_lowercase();
    let note = note.to_ascii_lowercase();
    if !valid_uuid(&todo) || !valid_uuid(&note) {
        return Err(invalid());
    }
    let name = format!("eggdone:task-note-link:v1:{todo}:{note}");
    Ok(Uuid::new_v5(&Uuid::NAMESPACE_DNS, name.as_bytes()).to_string())
}

pub fn validate_link(link: &TaskNoteLink) -> Result<(), String> {
    if !valid_uuid(&link.todo_uuid)
        || !valid_uuid(&link.note_uuid)
        || link.uuid != link_uuid(&link.todo_uuid, &link.note_uuid)?
        || !(0..=MAX_CLOCK).contains(&link.created_at)
        || !(link.created_at..=MAX_CLOCK).contains(&link.updated_at)
        || link.updated_by.is_empty()
        || link.updated_by.len() > 128
        || !link
            .updated_by
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
        || link
            .deleted_at
            .is_some_and(|t| t < link.created_at || t > link.updated_at)
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_document(doc: &LinkDocument) -> Result<(), String> {
    if doc.format_version != 1 || doc.links.len() > MAX_LINKS {
        return Err(invalid());
    }
    let mut ids = BTreeMap::new();
    for link in &doc.links {
        validate_link(link)?;
        if ids.insert(&link.uuid, ()).is_some() {
            return Err(invalid());
        }
    }
    Ok(())
}

fn normalize_numbers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => items.iter_mut().for_each(normalize_numbers),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(normalize_numbers),
        serde_json::Value::Number(number) => {
            if let Some(n) = number.as_f64() {
                if n.fract() == 0.0 && n.abs() <= MAX_CLOCK as f64 {
                    *value = serde_json::Value::Number((n as i64).into());
                }
            }
        }
        _ => (),
    }
}

pub fn parse_document(source: &str) -> Result<LinkDocument, String> {
    if source.encode_utf16().count() > MAX_DOCUMENT_UNITS {
        return Err(invalid());
    }
    let mut raw: serde_json::Value = serde_json::from_str(source).map_err(|_| invalid())?;
    normalize_numbers(&mut raw);
    for link in raw["links"].as_array().ok_or_else(invalid)? {
        if link.get("deleted_at").is_none() {
            return Err(invalid());
        }
    }
    let doc: LinkDocument = serde_json::from_value(raw).map_err(|_| invalid())?;
    validate_document(&doc)?;
    Ok(doc)
}

pub fn encode_document(doc: &LinkDocument) -> Result<String, String> {
    validate_document(doc)?;
    let mut doc = doc.clone();
    doc.links.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    let text = serde_json::to_string(&doc).map_err(|_| invalid())?;
    if text.len() > MAX_DOCUMENT_UNITS {
        return Err(invalid());
    }
    Ok(text)
}

fn rank(link: &TaskNoteLink) -> (i64, &str, bool, i64, i64) {
    (
        link.updated_at,
        &link.updated_by,
        link.deleted_at.is_some(),
        link.deleted_at.unwrap_or(-1),
        link.created_at,
    )
}

pub fn merge_documents(a: &LinkDocument, b: &LinkDocument) -> Result<LinkDocument, String> {
    validate_document(a)?;
    validate_document(b)?;
    let mut links: BTreeMap<String, TaskNoteLink> = BTreeMap::new();
    for candidate in a.links.iter().chain(&b.links) {
        if let Some(previous) = links.get(&candidate.uuid) {
            if previous.todo_uuid != candidate.todo_uuid
                || previous.note_uuid != candidate.note_uuid
            {
                return Err("TASK_NOTE_LINK_IDENTITY_CONFLICT".into());
            }
            if rank(previous) >= rank(candidate) {
                continue;
            }
        }
        links.insert(candidate.uuid.clone(), candidate.clone());
    }
    let doc = LinkDocument {
        format_version: 1,
        links: links.into_values().collect(),
    };
    encode_document(&doc)?;
    Ok(doc)
}

pub fn object_key(todo_key: &str, occupied: &[String]) -> Result<String, String> {
    if todo_key.is_empty()
        || todo_key.trim() != todo_key
        || todo_key.len() > 1024
        || todo_key
            .chars()
            .any(|c| c == '\\' || c <= '\u{1f}' || c == '\u{7f}')
        || todo_key
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("INVALID_TASK_NOTE_LINK_OBJECT_KEY".into());
    }
    let directory = todo_key.rfind('/').map_or("", |i| &todo_key[..=i]);
    let key = format!("{directory}task-note-links.json");
    if key == todo_key || key.len() > 1024 || occupied.contains(&key) {
        return Err("TASK_NOTE_LINK_OBJECT_KEY_COLLISION".into());
    }
    Ok(key)
}
