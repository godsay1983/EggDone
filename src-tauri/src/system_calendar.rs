//! Frozen system-calendar v1 wire. This is never a task or a backup document.
use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const INVALID: &str = "CALENDAR_INVALID_DOCUMENT";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CalendarDocument {
    pub format_version: u8,
    pub owner_id: String,
    pub owner_generation: String,
    pub revision: i64,
    pub operation_id: String,
    pub state: CalendarStatus,
    pub captured_at: i64,
    pub source_timezone: String,
    #[serde(deserialize_with = "required_nullable")]
    pub coverage: Option<CalendarCoverage>,
    pub calendars: Vec<CalendarSource>,
    pub occurrences: Vec<CalendarOccurrence>,
}

fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CalendarStatus {
    Active,
    Withdrawn,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CalendarCoverage {
    pub start: String,
    pub end: String,
    pub start_time: i64,
    pub end_time: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CalendarSource {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CalendarOccurrence {
    pub id: String,
    pub calendar_id: String,
    pub title: String,
    pub start_time: i64,
    pub end_time: i64,
    pub is_all_day: bool,
    pub start_date: String,
    pub end_date_exclusive: String,
    pub time_zone: String,
    pub location: String,
}

pub fn object_key(todo_key: &str) -> String {
    format!(
        "eggdone-calendar/v1/{:x}/snapshot.json",
        Sha256::digest(todo_key.as_bytes())
    )
}

pub fn parse(bytes: &[u8]) -> Result<CalendarDocument, String> {
    if bytes.len() > MAX_BYTES {
        return Err("CALENDAR_DOCUMENT_TOO_LARGE".into());
    }
    let document: CalendarDocument = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    validate(&document)?;
    Ok(document)
}

fn safe(value: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
}

fn timestamp(value: i64) -> bool {
    // Match ECMAScript Date's finite timestamp range on the publishing client.
    (-8_640_000_000_000_000..=8_640_000_000_000_000).contains(&value)
}

fn text(value: &str, limit: usize) -> bool {
    value.encode_utf16().count() <= limit
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && uuid::Uuid::parse_str(value)
            .is_ok_and(|id| id.hyphenated().to_string().eq_ignore_ascii_case(value))
}

fn date(value: &str) -> Result<time::Date, String> {
    if value.len() != 10
        || !value.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err(INVALID.into());
    }
    let year: i32 = value[..4].parse().map_err(|_| INVALID)?;
    let month: u8 = value[5..7].parse().map_err(|_| INVALID)?;
    let day: u8 = value[8..].parse().map_err(|_| INVALID)?;
    if year == 0 {
        return Err(INVALID.into());
    }
    time::Date::from_calendar_date(year, month.try_into().map_err(|_| INVALID)?, day)
        .map_err(|_| INVALID.into())
}

fn midnight(value: time::Date) -> i64 {
    value.midnight().assume_utc().unix_timestamp() * 1000
}

pub(crate) fn validate(doc: &CalendarDocument) -> Result<(), String> {
    if doc.format_version != 1 {
        return Err("CALENDAR_UNSUPPORTED_VERSION".into());
    }
    if !uuid(&doc.owner_id)
        || !uuid(&doc.owner_generation)
        || !uuid(&doc.operation_id)
        || doc.revision <= 0
        || !safe(doc.revision)
        || !timestamp(doc.captured_at)
        || !text(&doc.source_timezone, 128)
        || doc.occurrences.len() > 10_000
    {
        return Err(INVALID.into());
    }
    if doc.state == CalendarStatus::Withdrawn {
        return if doc.captured_at == 0
            && doc.source_timezone.is_empty()
            && doc.coverage.is_none()
            && doc.calendars.is_empty()
            && doc.occurrences.is_empty()
        {
            Ok(())
        } else {
            Err(INVALID.into())
        };
    }
    let coverage = doc.coverage.as_ref().ok_or(INVALID)?;
    let start = date(&coverage.start)?;
    let end = date(&coverage.end)?;
    if doc.captured_at <= 0
        || doc.source_timezone.is_empty()
        || (end - start).whole_days() != 211
        || !timestamp(coverage.start_time)
        || !timestamp(coverage.end_time)
        || coverage.end_time <= coverage.start_time
    {
        return Err(INVALID.into());
    }
    // Source-local boundaries may cross DST. Validate their civil dates and midnight, not 211*24h.
    let zone = jiff::tz::TimeZone::get(&doc.source_timezone).map_err(|_| INVALID)?;
    for (value, expected) in [
        (coverage.start_time, &coverage.start),
        (coverage.end_time, &coverage.end),
    ] {
        let instant = jiff::Timestamp::from_millisecond(value).map_err(|_| INVALID)?;
        let civil = instant.to_zoned(zone.clone());
        if civil.date().to_string() != *expected || civil.time() != jiff::civil::Time::midnight() {
            return Err(INVALID.into());
        }
    }
    let mut calendars = HashSet::new();
    for calendar in &doc.calendars {
        if !hash(&calendar.id) || !text(&calendar.title, 5000) || !calendars.insert(&calendar.id) {
            return Err(INVALID.into());
        }
    }
    let mut ids = HashSet::new();
    for occurrence in &doc.occurrences {
        if !hash(&occurrence.id)
            || !calendars.contains(&occurrence.calendar_id)
            || !ids.insert(&occurrence.id)
            || !text(&occurrence.title, 5000)
            || !text(&occurrence.location, 5000)
            || !text(&occurrence.time_zone, 128)
            || occurrence.time_zone.is_empty()
            || !timestamp(occurrence.start_time)
            || !timestamp(occurrence.end_time)
            || occurrence.end_time < occurrence.start_time
        {
            return Err(INVALID.into());
        }
        if occurrence.is_all_day {
            let first = date(&occurrence.start_date)?;
            let last = date(&occurrence.end_date_exclusive)?;
            if last <= first
                || midnight(first) != occurrence.start_time
                || midnight(last) != occurrence.end_time
                || first >= end
                || last <= start
            {
                return Err(INVALID.into());
            }
        } else {
            if !occurrence.start_date.is_empty()
                || !occurrence.end_date_exclusive.is_empty()
                || occurrence.start_time >= coverage.end_time
                || if occurrence.end_time == occurrence.start_time {
                    occurrence.start_time < coverage.start_time
                } else {
                    occurrence.end_time <= coverage.start_time
                }
            {
                return Err(INVALID.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "system_calendar_tests.rs"]
mod tests;
