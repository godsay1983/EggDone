use crate::task_checklist_protocol::valid_uuid;
pub use crate::task_checklist_protocol::MAX_CLOCK;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::io::{self, Write};

pub const MAX_ROWS: usize = 20_000;
pub const MAX_BYTES: usize = 16_777_216;
pub const MAX_BODY_UNITS: usize = 1000;

fn invalid() -> String {
    "PROGRESS_INVALID".into()
}

fn limit() -> String {
    "PROGRESS_LIMIT".into()
}

// A deserialize hook makes nullable fields required, unlike serde's default Option.
fn required_nullable<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub uuid: String,
    pub task_uuid: String,
    pub body: String,
    pub created_at: i64,
    pub created_by: String,
    pub updated_at: i64,
    pub updated_by: String,
    pub clock: i64,
    #[serde(deserialize_with = "required_nullable")]
    pub deleted_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: u32,
    pub entries: Vec<Entry>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format_version: 1,
            entries: Vec::new(),
        }
    }
}

fn trim_space(c: char) -> bool {
    matches!(c as u32,
        0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a |
        0x2028..=0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

fn valid_body(body: &str) -> bool {
    !body.is_empty()
        && body.encode_utf16().count() <= MAX_BODY_UNITS
        && body.trim_matches(trim_space) == body
        && body.chars().all(|c| {
            let n = c as u32;
            !(n <= 31 && !matches!(n, 9 | 10 | 13)
                || (127..=159).contains(&n)
                || (0x202a..=0x202e).contains(&n)
                || (0x2066..=0x2069).contains(&n))
        })
}

pub fn normalize_body(body: &str) -> Result<String, String> {
    let body = body.trim_matches(trim_space);
    if body.encode_utf16().count() > MAX_BODY_UNITS {
        return Err(limit());
    }
    if !valid_body(body) {
        return Err(invalid());
    }
    Ok(body.to_owned())
}

pub fn next_clock(clock: i64) -> Result<i64, String> {
    if !(0..=MAX_CLOCK).contains(&clock) {
        return Err(invalid());
    }
    if clock == MAX_CLOCK {
        return Err(limit());
    }
    Ok(clock + 1)
}

fn valid_device(device: &str) -> bool {
    !device.is_empty()
        && device.len() <= 128
        && device
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

pub fn validate_entry(entry: &Entry) -> Result<(), String> {
    if !valid_uuid(&entry.uuid)
        || !valid_uuid(&entry.task_uuid)
        || !valid_device(&entry.created_by)
        || !valid_device(&entry.updated_by)
        || !(0..=MAX_CLOCK).contains(&entry.created_at)
        || !(entry.created_at..=MAX_CLOCK).contains(&entry.updated_at)
        || !(1..=MAX_CLOCK).contains(&entry.clock)
    {
        return Err(invalid());
    }
    match entry.deleted_at {
        None if entry.body.encode_utf16().count() > MAX_BODY_UNITS => Err(limit()),
        None if valid_body(&entry.body) => Ok(()),
        Some(deleted) if entry.body.is_empty() && deleted == entry.updated_at => Ok(()),
        _ => Err(invalid()),
    }
}

#[derive(Serialize)]
struct CanonicalDocument<'a> {
    format_version: u32,
    entries: Vec<&'a Entry>,
}

fn canonical(document: &Document) -> CanonicalDocument<'_> {
    let mut entries: Vec<_> = document.entries.iter().collect();
    entries.sort_unstable_by(|a, b| a.uuid.cmp(&b.uuid));
    CanonicalDocument {
        format_version: document.format_version,
        entries,
    }
}

// Measure escaped JSON without allocating an oversized document.
#[derive(Default)]
struct SizeWriter {
    bytes: usize,
}

impl Write for SizeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES - self.bytes {
            return Err(io::Error::other("PROGRESS_LIMIT"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn validate(document: &Document) -> Result<(), String> {
    if document.format_version != 1 {
        return Err(invalid());
    }
    if document.entries.len() > MAX_ROWS {
        return Err(limit());
    }
    let mut ids = HashSet::with_capacity(document.entries.len());
    for entry in &document.entries {
        validate_entry(entry)?;
        if !ids.insert(&entry.uuid) {
            return Err(invalid());
        }
    }
    serde_json::to_writer(SizeWriter::default(), &canonical(document)).map_err(|_| limit())
}

pub fn encode(document: &Document) -> Result<String, String> {
    validate(document)?;
    serde_json::to_string(&canonical(document)).map_err(|_| invalid())
}

// serde_json's decimal/exponent numbers use f64. Check their exact decimal value
// before normalization so a fractional or out-of-range token cannot round valid.
fn integer_token(token: &str) -> Result<i64, String> {
    serde_json::from_str::<serde_json::Number>(token).map_err(|_| invalid())?;
    if token.starts_with('-') {
        return Err(invalid());
    }
    let (mantissa, exponent) = match token.find(['e', 'E']) {
        Some(index) => {
            let exponent = token[index + 1..].parse::<i64>().map_err(|_| invalid())?;
            (&token[..index], exponent)
        }
        None => (token, 0),
    };
    let fraction = mantissa
        .find('.')
        .map_or(0, |index| mantissa.len() - index - 1);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Ok(0);
    }
    let significant = digits.trim_end_matches('0');
    let scale = exponent
        .checked_sub(fraction as i64)
        .and_then(|n| n.checked_add((digits.len() - significant.len()) as i64))
        .ok_or_else(invalid)?;
    if scale < 0 || scale > 15 || significant.len() + scale as usize > 16 {
        return Err(invalid());
    }
    let value = significant
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(10_i64.pow(scale as u32)))
        .filter(|n| *n <= MAX_CLOCK)
        .ok_or_else(invalid)?;
    Ok(value)
}

fn normalize_numbers(source: &str) -> Result<String, String> {
    let bytes = source.as_bytes();
    let mut result = String::with_capacity(source.len());
    let mut index = 0;
    let mut copied = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                result.push_str(&source[copied..start]);
                result.push_str(&integer_token(&source[start..index])?.to_string());
                copied = index;
            }
            _ => index += 1,
        }
    }
    result.push_str(&source[copied..]);
    Ok(result)
}

pub fn parse(source: &str) -> Result<Document, String> {
    if source.len() > MAX_BYTES {
        return Err(limit());
    }
    let mut document: Document =
        serde_json::from_str(&normalize_numbers(source)?).map_err(|_| invalid())?;
    validate(&document)?;
    document
        .entries
        .sort_unstable_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(document)
}

fn compare(a: &Entry, b: &Entry) -> Ordering {
    a.deleted_at
        .is_some()
        .cmp(&b.deleted_at.is_some())
        .then_with(|| a.clock.cmp(&b.clock))
        .then_with(|| a.updated_by.cmp(&b.updated_by))
        .then_with(|| a.body.encode_utf16().cmp(b.body.encode_utf16()))
        .then_with(|| a.updated_at.cmp(&b.updated_at))
}

pub fn merge(left: &Document, right: &Document) -> Result<Document, String> {
    validate(left)?;
    validate(right)?;
    let mut entries: BTreeMap<&str, &Entry> = BTreeMap::new();
    for entry in left.entries.iter().chain(&right.entries) {
        if let Some(previous) = entries.get(entry.uuid.as_str()) {
            if previous.task_uuid != entry.task_uuid
                || previous.created_at != entry.created_at
                || previous.created_by != entry.created_by
            {
                return Err("PROGRESS_CONFLICT".into());
            }
            if compare(previous, entry) != Ordering::Less {
                continue;
            }
        }
        entries.insert(&entry.uuid, entry);
    }
    let document = Document {
        format_version: 1,
        entries: entries.into_values().cloned().collect(),
    };
    validate(&document)?;
    Ok(document)
}

pub fn filter_purged(
    document: &Document,
    task_uuids: &HashSet<String>,
) -> Result<Document, String> {
    validate(document)?;
    if task_uuids.iter().any(|uuid| !valid_uuid(uuid)) {
        return Err(invalid());
    }
    let mut filtered = Document {
        format_version: 1,
        entries: document
            .entries
            .iter()
            .filter(|entry| !task_uuids.contains(&entry.task_uuid))
            .cloned()
            .collect(),
    };
    filtered
        .entries
        .sort_unstable_by(|a, b| a.uuid.cmp(&b.uuid));
    validate(&filtered)?;
    Ok(filtered)
}

pub fn object_key(todo_key: &str, occupied: &[String]) -> Result<String, String> {
    // The existing function exposes path validation together with link derivation.
    // A link-path collision still means the input task path itself is valid.
    if todo_key.trim_matches(trim_space) != todo_key
        || crate::task_note_link_protocol::object_key(todo_key, &[])
            .is_err_and(|error| error != "TASK_NOTE_LINK_OBJECT_KEY_COLLISION")
    {
        return Err("PROGRESS_KEY_INVALID".into());
    }
    let digest = Sha256::digest(todo_key.as_bytes());
    let key = format!("eggdone-progress/v1/{digest:x}/entries.json");
    if key == todo_key || occupied.contains(&key) {
        return Err("PROGRESS_KEY_COLLISION".into());
    }
    Ok(key)
}

#[cfg(test)]
#[path = "task_progress_protocol_tests.rs"]
mod tests;
