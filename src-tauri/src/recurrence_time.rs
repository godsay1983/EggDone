//! Custom recurrence only; date-only values are not converted to device-local midnight.
use jiff::{civil::DateTime, tz};

pub fn recurrence_due_at(
    date: &str,
    minutes: Option<u16>,
    timezone_id: Option<&str>,
) -> Result<Option<i64>, String> {
    let invalid = || "INVALID_RECURRENCE_TIME".to_string();
    if date.len() != 10
        || !date.bytes().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return Err(invalid());
    }
    let civil_date: jiff::civil::Date = date.parse().map_err(|_| invalid())?;
    if civil_date.year() < 1900 {
        return Err(invalid());
    }
    let (minutes, zone_id) = match (minutes, timezone_id) {
        (None, None) => return Ok(None),
        (Some(m), Some(z)) if m < 1440 => (m, z),
        _ => return Err(invalid()),
    };
    // Pin the shipped IANA database on every desktop OS; never use system-zone fallback.
    let database = tz::TimeZoneDatabase::bundled();
    if !database.available().any(|id| id.to_string() == zone_id) {
        return Err("RECURRENCE_TIMEZONE_UNSUPPORTED".to_string());
    }
    let zone = database
        .get(zone_id)
        .map_err(|_| "RECURRENCE_TIMEZONE_UNSUPPORTED".to_string())?;
    let civil = DateTime::new(
        civil_date.year(),
        civil_date.month(),
        civil_date.day(),
        (minutes / 60) as i8,
        (minutes % 60) as i8,
        0,
        0,
    )
    .map_err(|_| invalid())?;
    let zoned = zone
        .to_ambiguous_zoned(civil)
        .compatible()
        .map_err(|_| invalid())?;
    Ok(Some(zoned.timestamp().as_millisecond()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_recurrence_timezone_fixtures() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/fixtures/recurrence-time-v1.json"))
                .unwrap();
        for case in fixtures.as_array().unwrap() {
            let minutes = case["minutes"].as_u64().map(|n| n as u16);
            let result = recurrence_due_at(
                case["date"].as_str().unwrap(),
                minutes,
                case["zone"].as_str(),
            );
            if case["error"].as_bool() == Some(true) {
                assert!(result.is_err(), "{}", case["id"]);
            } else {
                let expected = case["utc"]
                    .as_str()
                    .map(|v| v.parse::<jiff::Timestamp>().unwrap().as_millisecond());
                assert_eq!(result.unwrap(), expected, "{}", case["id"]);
            }
        }
    }
}
