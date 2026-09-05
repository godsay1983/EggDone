//! Calendar foundation only; not yet wired to persistence, sync or task completion.
use serde::{Deserialize, Serialize};
use time::{Date, Duration, Month};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecurrenceSchedule {
    pub anchor_date: String,
    pub frequency: String,
    pub interval: u32,
    pub weekdays: Vec<u8>,
    pub month_day: Option<u8>,
    pub end_type: String,
    pub end_date: Option<String>,
    pub max_occurrences: Option<u32>,
    pub local_time_minutes: Option<u16>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RecurrenceOccurrence {
    pub date: String,
    pub index: u32,
}

fn invalid() -> String {
    "INVALID_RECURRENCE".to_string()
}

fn parse_date(value: &str) -> Result<Date, String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(invalid());
    }
    let year = value[..4].parse::<i32>().map_err(|_| invalid())?;
    let month = value[5..7].parse::<u8>().map_err(|_| invalid())?;
    let day = value[8..].parse::<u8>().map_err(|_| invalid())?;
    if !(1900..=9999).contains(&year) {
        return Err(invalid());
    }
    Date::from_calendar_date(year, Month::try_from(month).map_err(|_| invalid())?, day)
        .map_err(|_| invalid())
}

fn date_only(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn month_day(year: i32, month: Month, requested: u8) -> Result<u8, String> {
    let last = (28..=31)
        .rev()
        .find(|day| Date::from_calendar_date(year, month, *day).is_ok())
        .ok_or_else(invalid)?;
    Ok(if requested == 0 {
        last
    } else {
        requested.min(last)
    })
}

pub fn validate_recurrence(schedule: &RecurrenceSchedule) -> Result<(), String> {
    let anchor = parse_date(&schedule.anchor_date)?;
    if !(1..=99).contains(&schedule.interval)
        || schedule.local_time_minutes.is_some_and(|m| m > 1439)
    {
        return Err(invalid());
    }
    match schedule.frequency.as_str() {
        "weekly" => {
            if schedule.weekdays.is_empty()
                || schedule.weekdays.len() > 7
                || schedule.month_day.is_some()
                || !schedule
                    .weekdays
                    .contains(&anchor.weekday().number_from_monday())
                || schedule.weekdays.iter().any(|d| !(1..=7).contains(d))
                || schedule.weekdays.windows(2).any(|days| days[0] >= days[1])
            {
                return Err(invalid());
            }
        }
        "monthly" => {
            let requested = schedule.month_day.ok_or_else(invalid)?;
            if requested > 31
                || !schedule.weekdays.is_empty()
                || anchor.day() != month_day(anchor.year(), anchor.month(), requested)?
            {
                return Err(invalid());
            }
        }
        "daily" if schedule.weekdays.is_empty() && schedule.month_day.is_none() => (),
        _ => return Err(invalid()),
    }
    match schedule.end_type.as_str() {
        "date" => {
            let end = parse_date(schedule.end_date.as_deref().ok_or_else(invalid)?)?;
            if end < anchor || schedule.max_occurrences.is_some() {
                return Err(invalid());
            }
        }
        "count" => {
            if schedule.end_date.is_some()
                || !schedule
                    .max_occurrences
                    .is_some_and(|count| (1..=100000).contains(&count))
            {
                return Err(invalid());
            }
        }
        "never" if schedule.end_date.is_none() && schedule.max_occurrences.is_none() => (),
        _ => return Err(invalid()),
    }
    Ok(())
}

// Do not trust generated_count or the device clock to assign occurrence identities.
fn occurrence_index(schedule: &RecurrenceSchedule, current: Date) -> Result<u32, String> {
    let anchor = parse_date(&schedule.anchor_date)?;
    let days = (current - anchor).whole_days();
    if days < 0 {
        return Err(invalid());
    }
    let interval = i64::from(schedule.interval);
    let index = match schedule.frequency.as_str() {
        "daily" => {
            if days % interval != 0 {
                return Err(invalid());
            }
            days / interval + 1
        }
        "monthly" => {
            let months = i64::from(
                (current.year() - anchor.year()) * 12 + current.month() as i32
                    - anchor.month() as i32,
            );
            if months % interval != 0
                || current.day()
                    != month_day(
                        current.year(),
                        current.month(),
                        schedule.month_day.ok_or_else(invalid)?,
                    )?
            {
                return Err(invalid());
            }
            months / interval + 1
        }
        "weekly" => {
            let anchor_day = anchor.weekday().number_from_monday();
            let weeks = (days + i64::from(anchor_day) - 1) / 7;
            let position = schedule
                .weekdays
                .iter()
                .position(|d| *d == current.weekday().number_from_monday())
                .ok_or_else(invalid)?;
            if weeks % interval != 0 {
                return Err(invalid());
            }
            let before = schedule
                .weekdays
                .iter()
                .filter(|d| **d < anchor_day)
                .count();
            weeks / interval * schedule.weekdays.len() as i64 + position as i64 + 1 - before as i64
        }
        _ => return Err(invalid()),
    };
    u32::try_from(index).map_err(|_| invalid())
}

fn allowed(schedule: &RecurrenceSchedule, date: &str, index: u32) -> bool {
    schedule.end_date.as_deref().is_none_or(|end| date <= end)
        && schedule.max_occurrences.is_none_or(|max| index <= max)
}

pub fn next_recurrence(
    schedule: &RecurrenceSchedule,
    current_date: &str,
) -> Result<Option<RecurrenceOccurrence>, String> {
    validate_recurrence(schedule)?;
    let current = parse_date(current_date)?;
    let index = occurrence_index(schedule, current)?;
    if !allowed(schedule, current_date, index) {
        return Err(invalid());
    }
    if schedule.max_occurrences.is_some_and(|max| index >= max) {
        return Ok(None);
    }
    let next = match schedule.frequency.as_str() {
        "daily" => current.checked_add(Duration::days(i64::from(schedule.interval))),
        "monthly" => {
            let month_index =
                current.year() * 12 + current.month() as i32 - 1 + schedule.interval as i32;
            let year = month_index / 12;
            if year > 9999 {
                return Ok(None);
            }
            let month = Month::try_from((month_index % 12 + 1) as u8).map_err(|_| invalid())?;
            Some(
                Date::from_calendar_date(
                    year,
                    month,
                    month_day(year, month, schedule.month_day.ok_or_else(invalid)?)?,
                )
                .map_err(|_| invalid())?,
            )
        }
        "weekly" => {
            let anchor = parse_date(&schedule.anchor_date)?;
            let mut next = None;
            for step in 1..=schedule.interval * 7 {
                let Some(candidate) = current.checked_add(Duration::days(i64::from(step))) else {
                    break;
                };
                let weeks = ((candidate - anchor).whole_days()
                    + i64::from(anchor.weekday().number_from_monday())
                    - 1)
                    / 7;
                if weeks % i64::from(schedule.interval) == 0
                    && schedule
                        .weekdays
                        .contains(&candidate.weekday().number_from_monday())
                {
                    next = Some(candidate);
                    break;
                }
            }
            next
        }
        _ => return Err(invalid()),
    };
    let Some(next) = next else {
        return Ok(None);
    };
    if next.year() > 9999 {
        return Ok(None);
    }
    let date = date_only(next);
    Ok(
        allowed(schedule, &date, index + 1).then_some(RecurrenceOccurrence {
            date,
            index: index + 1,
        }),
    )
}

pub fn recurrence_occurrence_key(
    rule_uuid: &str,
    schedule: &RecurrenceSchedule,
    occurrence: &RecurrenceOccurrence,
) -> Result<String, String> {
    validate_recurrence(schedule)?;
    let uuid = Uuid::parse_str(rule_uuid).map_err(|_| invalid())?;
    let version = uuid.get_version_num();
    if uuid.to_string() != rule_uuid
        || !(1..=5).contains(&version)
        || uuid.get_variant() != uuid::Variant::RFC4122
        || occurrence.index != occurrence_index(schedule, parse_date(&occurrence.date)?)?
        || !allowed(schedule, &occurrence.date, occurrence.index)
    {
        return Err(invalid());
    }
    let clock = schedule.local_time_minutes.map_or_else(
        || "date".to_string(),
        |minutes| format!("{:02}:{:02}", minutes / 60, minutes % 60),
    );
    Ok(format!(
        "eggdone/recurrence/v1/{rule_uuid}/{}T{clock}/{}",
        occurrence.date, occurrence.index
    ))
}

pub fn recurrence_todo_uuid(
    rule_uuid: &str,
    schedule: &RecurrenceSchedule,
    occurrence: &RecurrenceOccurrence,
) -> Result<String, String> {
    let key = recurrence_occurrence_key(rule_uuid, schedule, occurrence)?;
    Ok(Uuid::new_v5(&Uuid::NAMESPACE_DNS, key.as_bytes()).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Fixture {
        id: String,
        schedule: RecurrenceSchedule,
        current_date: String,
        expected: Option<RecurrenceOccurrence>,
        key: Option<String>,
        uuid: Option<String>,
        #[serde(default)]
        error: bool,
    }

    #[test]
    fn shared_calendar_and_identity_fixtures() {
        let fixtures: Vec<Fixture> =
            serde_json::from_str(include_str!("../../docs/fixtures/recurrence-v1.json")).unwrap();
        for fixture in fixtures {
            let result = next_recurrence(&fixture.schedule, &fixture.current_date);
            if fixture.error {
                assert!(result.is_err(), "{}", fixture.id);
                continue;
            }
            let result = result.unwrap_or_else(|error| panic!("{}: {error}", fixture.id));
            assert_eq!(result, fixture.expected, "{}", fixture.id);
            if let Some(occurrence) = result {
                let rule = "123e4567-e89b-42d3-a456-426614174000";
                assert_eq!(
                    recurrence_occurrence_key(rule, &fixture.schedule, &occurrence).unwrap(),
                    fixture.key.unwrap(),
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    recurrence_todo_uuid(rule, &fixture.schedule, &occurrence).unwrap(),
                    fixture.uuid.unwrap(),
                    "{}",
                    fixture.id
                );
            }
        }
    }

    #[test]
    fn rejects_noncanonical_identity_and_forged_index() {
        let fixture: Vec<Fixture> =
            serde_json::from_str(include_str!("../../docs/fixtures/recurrence-v1.json")).unwrap();
        let schedule = &fixture[0].schedule;
        let mut occurrence = next_recurrence(schedule, &fixture[0].current_date)
            .unwrap()
            .unwrap();
        assert!(recurrence_todo_uuid(
            "123E4567-E89B-42D3-A456-426614174000",
            schedule,
            &occurrence
        )
        .is_err());
        occurrence.index += 1;
        assert!(recurrence_todo_uuid(
            "123e4567-e89b-42d3-a456-426614174000",
            schedule,
            &occurrence
        )
        .is_err());
    }
}
