use crate::task_checklist_protocol::{valid_text, valid_uuid, MAX_CLOCK};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BYTES: usize = 4 * 1024 * 1024;
fn invalid() -> String {
    "INVALID_TEMPLATE_DOCUMENT".into()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateContent {
    pub name: String,
    pub title: String,
    pub note: String,
    pub group_uuid: Option<String>,
    pub checklist: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskTemplate {
    pub uuid: String,
    pub content: TemplateContent,
    pub created_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatesDocument {
    pub format_version: u32,
    pub templates: Vec<TaskTemplate>,
}
impl Default for TemplatesDocument {
    fn default() -> Self {
        Self {
            format_version: 1,
            templates: vec![],
        }
    }
}
fn text(s: &str, limit: usize) -> bool {
    !s.is_empty()
        && s.trim() == s
        && !s.starts_with('\u{feff}')
        && !s.ends_with('\u{feff}')
        && valid_text(s, limit, false)
}
pub fn validate_content(c: &TemplateContent) -> Result<(), String> {
    if !text(&c.name, 60)
        || !text(&c.title, 100)
        || !valid_text(&c.note, 1000, true)
        || c.group_uuid.as_ref().is_some_and(|g| !valid_uuid(g))
        || c.checklist.len() > 1000
        || c.checklist.iter().any(|s| !text(s, 200))
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn validate(row: &TaskTemplate) -> Result<(), String> {
    validate_content(&row.content)?;
    if !valid_uuid(&row.uuid)
        || !(0..=MAX_CLOCK).contains(&row.created_at)
        || !(row.created_at..=MAX_CLOCK).contains(&row.updated_at)
        || row.updated_by.is_empty()
        || row.updated_by.len() > 128
        || !row
            .updated_by
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        || row
            .deleted_at
            .is_some_and(|d| !(row.created_at..=row.updated_at).contains(&d))
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn encode(doc: &TemplatesDocument) -> Result<String, String> {
    if doc.format_version != 1 || doc.templates.len() > 1000 {
        return Err(invalid());
    }
    let mut copy = doc.clone();
    let mut ids = BTreeSet::new();
    let mut count = 0;
    for row in &copy.templates {
        validate(row)?;
        count += row.content.checklist.len();
        if !ids.insert(&row.uuid) || count > 20000 {
            return Err(invalid());
        }
    }
    copy.templates.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    let raw = serde_json::to_string(&copy).map_err(|_| invalid())?;
    if raw.len() > MAX_BYTES {
        return Err(invalid());
    }
    Ok(raw)
}
pub fn parse(raw: &str) -> Result<TemplatesDocument, String> {
    if raw.len() > MAX_BYTES {
        return Err(invalid());
    }
    // Option fields must be present as explicit nulls on the wire, just as in ArkTS.
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    if let Some(rows) = value.get("templates").and_then(|v| v.as_array()) {
        for row in rows {
            if row.get("deleted_at").is_none()
                || row
                    .get("content")
                    .and_then(|c| c.get("group_uuid"))
                    .is_none()
            {
                return Err(invalid());
            }
        }
    }
    let mut doc: TemplatesDocument = serde_json::from_str(raw).map_err(|_| invalid())?;
    encode(&doc)?;
    doc.templates.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(doc)
}
pub fn merge(a: &TemplatesDocument, b: &TemplatesDocument) -> Result<TemplatesDocument, String> {
    encode(a)?;
    encode(b)?;
    let mut rows: BTreeMap<String, TaskTemplate> = BTreeMap::new();
    for row in a.templates.iter().chain(&b.templates) {
        if let Some(old) = rows.get(&row.uuid) {
            if old.created_at != row.created_at {
                return Err("TEMPLATE_IDENTITY_CONFLICT".into());
            }
            let old_json = serde_json::to_string(&old.content).map_err(|_| invalid())?;
            let new_json = serde_json::to_string(&row.content).map_err(|_| invalid())?;
            if (
                old.deleted_at.is_some(),
                old.updated_at,
                &old.updated_by,
                old.deleted_at,
                old_json,
            ) >= (
                row.deleted_at.is_some(),
                row.updated_at,
                &row.updated_by,
                row.deleted_at,
                new_json,
            ) {
                continue;
            }
        }
        rows.insert(row.uuid.clone(), row.clone());
    }
    let doc = TemplatesDocument {
        format_version: 1,
        templates: rows.into_values().collect(),
    };
    encode(&doc)?;
    Ok(doc)
}
