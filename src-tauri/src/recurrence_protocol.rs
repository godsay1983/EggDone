use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::{Uuid, Variant};

use crate::recurrence::{
    next_recurrence, recurrence_todo_uuid, RecurrenceOccurrence, RecurrenceSchedule,
};

pub const MAX_RULES: usize = 2000;
pub const MAX_DOCUMENT_UNITS: usize = 1_048_576;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecurrenceRule {
    pub uuid: String,
    pub first_todo_uuid: String,
    pub schedule: RecurrenceSchedule,
    pub timezone_id: Option<String>,
    pub current_todo_uuid: String,
    pub current_date: String,
    pub generated_count: u32,
    pub exhausted: bool,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecurrenceDocument {
    pub format_version: u32,
    pub rules: Vec<RecurrenceRule>,
}

fn invalid() -> String {
    "INVALID_RECURRENCE_DOCUMENT".to_string()
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| {
        uuid.to_string() == value
            && (1..=5).contains(&uuid.get_version_num())
            && uuid.get_variant() == Variant::RFC4122
    })
}

pub fn validate_rule(rule: &RecurrenceRule) -> Result<(), String> {
    if !valid_uuid(&rule.uuid)
        || !valid_uuid(&rule.first_todo_uuid)
        || !valid_uuid(&rule.current_todo_uuid)
        || !(0..=MAX_SAFE_INTEGER).contains(&rule.updated_at)
        || rule.updated_by.is_empty()
        || rule.updated_by.len() > 128
        || !rule
            .updated_by
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
        || rule
            .deleted_at
            .is_some_and(|time| time < 0 || time > rule.updated_at)
    {
        return Err(invalid());
    }
    match (&rule.timezone_id, rule.schedule.local_time_minutes) {
        (None, None) => (),
        (Some(zone), Some(_))
            if !zone.is_empty()
                && zone.len() <= 128
                && zone
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_+-/".contains(&c))
                && !zone.starts_with('/')
                && !zone.ends_with('/')
                && !zone.contains("//") =>
        {
            ()
        }
        _ => return Err(invalid()),
    }
    let occurrence = RecurrenceOccurrence {
        date: rule.current_date.clone(),
        index: rule.generated_count,
    };
    let derived = recurrence_todo_uuid(&rule.uuid, &rule.schedule, &occurrence)?;
    if (rule.generated_count == 1 && rule.current_todo_uuid != rule.first_todo_uuid)
        || (rule.generated_count != 1 && rule.current_todo_uuid != derived)
        || (rule.exhausted && next_recurrence(&rule.schedule, &rule.current_date)?.is_some())
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_document(document: &RecurrenceDocument) -> Result<(), String> {
    if document.format_version != 1 || document.rules.len() > MAX_RULES {
        return Err(invalid());
    }
    let mut ids = BTreeMap::new();
    let mut active = BTreeMap::new();
    for rule in &document.rules {
        validate_rule(rule)?;
        if ids.insert(&rule.uuid, ()).is_some() {
            return Err(invalid());
        }
        if rule.deleted_at.is_none()
            && !rule.exhausted
            && active.insert(&rule.current_todo_uuid, ()).is_some()
        {
            return Err("RECURRENCE_LINK_CONFLICT".to_string());
        }
    }
    Ok(())
}

pub fn parse_document(source: &str) -> Result<RecurrenceDocument, String> {
    if source.encode_utf16().count() > MAX_DOCUMENT_UNITS {
        return Err(invalid());
    }
    // Nullable fields are required too; serde Option alone would silently accept missing fields.
    let mut value: serde_json::Value = serde_json::from_str(source).map_err(|_| invalid())?;
    normalize_integer_numbers(&mut value);
    let document: RecurrenceDocument =
        serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    for raw in value["rules"].as_array().ok_or_else(invalid)? {
        for key in ["timezone_id", "deleted_at"] {
            if raw.get(key).is_none() {
                return Err(invalid());
            }
        }
        for key in [
            "month_day",
            "end_date",
            "max_occurrences",
            "local_time_minutes",
        ] {
            if raw["schedule"].get(key).is_none() {
                return Err(invalid());
            }
        }
    }
    validate_document(&document)?;
    Ok(document)
}

fn normalize_integer_numbers(value: &mut serde_json::Value) {
    // JSON/ArkTS has one numeric type: 1.0 and 1e0 must behave like 1 on both clients.
    match value {
        serde_json::Value::Array(items) => items.iter_mut().for_each(normalize_integer_numbers),
        serde_json::Value::Object(fields) => {
            fields.values_mut().for_each(normalize_integer_numbers)
        }
        serde_json::Value::Number(number) => {
            if let Some(n) = number.as_f64() {
                if n.fract() == 0.0 && n.abs() <= MAX_SAFE_INTEGER as f64 {
                    *value = serde_json::Value::Number((n as i64).into());
                }
            }
        }
        _ => (),
    }
}

pub fn encode_document(document: &RecurrenceDocument) -> Result<String, String> {
    validate_document(document)?;
    let mut sorted = document.clone();
    sorted.rules.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    let source = serde_json::to_string(&sorted).map_err(|_| invalid())?;
    if source.len() > MAX_DOCUMENT_UNITS {
        return Err(invalid());
    }
    Ok(source)
}

pub fn merge_documents(
    local: &RecurrenceDocument,
    remote: &RecurrenceDocument,
) -> Result<RecurrenceDocument, String> {
    validate_document(local)?;
    validate_document(remote)?;
    let mut rules: BTreeMap<String, RecurrenceRule> = BTreeMap::new();
    for candidate in local.rules.iter().chain(&remote.rules) {
        if let Some(previous) = rules.get(&candidate.uuid) {
            if previous.schedule != candidate.schedule
                || previous.first_todo_uuid != candidate.first_todo_uuid
                || previous.timezone_id != candidate.timezone_id
            {
                return Err("RECURRENCE_IMMUTABLE_CONFLICT".to_string());
            }
            let rank = |r: &RecurrenceRule| {
                (
                    r.deleted_at.is_some(),
                    r.generated_count,
                    r.exhausted,
                    r.updated_at,
                    r.updated_by.clone(),
                    r.deleted_at,
                )
            };
            if rank(previous) >= rank(candidate) {
                continue;
            }
        }
        rules.insert(candidate.uuid.clone(), candidate.clone());
    }
    let merged = RecurrenceDocument {
        format_version: 1,
        rules: rules.into_values().collect(),
    };
    validate_document(&merged)?;
    Ok(merged)
}

pub fn recurrence_object_key(todo_key: &str, occupied_keys: &[String]) -> Result<String, String> {
    if todo_key.is_empty()
        || todo_key
            .trim_matches(|c: char| (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}')
            != todo_key
        || todo_key.len() > 1024
        || todo_key.contains('\\')
        || todo_key.bytes().any(|c| c < 32 || c == 127)
        || todo_key
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("INVALID_RECURRENCE_OBJECT_KEY".to_string());
    }
    let prefix = todo_key.rfind('/').map_or("", |index| &todo_key[..=index]);
    let key = format!("{prefix}recurrence-rules.json");
    if key == todo_key || occupied_keys.contains(&key) || key.len() > 1024 {
        return Err("RECURRENCE_OBJECT_KEY_COLLISION".to_string());
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../docs/fixtures/recurrence-document-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn shared_document_validation_and_roundtrip() {
        let fixtures = fixtures();
        for fixture in fixtures["valid"].as_array().unwrap() {
            let document = parse_document(&fixture["document"].to_string()).unwrap();
            assert_eq!(
                parse_document(&encode_document(&document).unwrap()).unwrap(),
                document
            );
        }
        for fixture in fixtures["invalid"].as_array().unwrap() {
            assert!(
                parse_document(&fixture["document"].to_string()).is_err(),
                "{}",
                fixture["id"]
            );
        }
        assert!(parse_document(&" ".repeat(MAX_DOCUMENT_UNITS + 1)).is_err());
        assert!(parse_document("{}").is_err());
        let numeric = fixtures["valid"][1]["document"]
            .to_string()
            .replace("\"interval\":1", "\"interval\":1e0");
        assert_eq!(
            parse_document(&numeric).unwrap().rules[0].schedule.interval,
            1
        );
    }

    #[test]
    fn shared_merge_is_commutative_and_idempotent() {
        for fixture in fixtures()["merges"].as_array().unwrap() {
            let left = parse_document(&fixture["left"].to_string()).unwrap();
            let right = parse_document(&fixture["right"].to_string()).unwrap();
            let result = merge_documents(&left, &right);
            if fixture["error"] == true {
                assert!(result.is_err(), "{}", fixture["id"]);
                assert!(merge_documents(&right, &left).is_err());
                continue;
            }
            let expected = parse_document(&fixture["expected"].to_string()).unwrap();
            let result = result.unwrap();
            assert_eq!(result, expected, "{}", fixture["id"]);
            assert_eq!(merge_documents(&right, &left).unwrap(), expected);
            assert_eq!(merge_documents(&result, &right).unwrap(), expected);
        }
    }

    #[test]
    fn shared_object_key_fixtures() {
        for fixture in fixtures()["keys"].as_array().unwrap() {
            let occupied: Vec<String> =
                serde_json::from_value(fixture["occupied"].clone()).unwrap();
            let result = recurrence_object_key(fixture["todo"].as_str().unwrap(), &occupied);
            if fixture["error"] == true {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), fixture["expected"].as_str().unwrap());
            }
        }
        assert_eq!(
            recurrence_object_key("\u{540c}\u{6b65}/todos.json", &[]).unwrap(),
            "\u{540c}\u{6b65}/recurrence-rules.json"
        );
        assert!(
            recurrence_object_key(&format!("{}/todos.json", "\u{4e2d}".repeat(340)), &[]).is_err()
        );
        assert!(recurrence_object_key("a\tb/todos.json", &[]).is_err());
    }
}
