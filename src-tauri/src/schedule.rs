use time::{Date, Month, OffsetDateTime, UtcOffset};

const DEFAULT_DUE_HOUR: u8 = 18;

pub(crate) fn local_date_from_timestamp(timestamp: i64) -> Result<String, String> {
    let local = local_datetime(timestamp)?;
    let (year, month, day) = local.date().to_calendar_date();
    Ok(format!("{year:04}-{:02}-{day:02}", month as u8))
}

pub(crate) fn timestamp_for_local_date(
    date: &str,
    source_timestamp: Option<i64>,
) -> Result<i64, String> {
    let date = parse_date(date)?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let (hour, minute) = match source_timestamp {
        Some(timestamp) => {
            let local = local_datetime(timestamp)?;
            (local.hour(), local.minute())
        }
        None => (DEFAULT_DUE_HOUR, 0),
    };
    let local = date
        .with_hms(hour, minute, 0)
        .map_err(|_| "到期时间无效".to_string())?;
    let millis = local.assume_offset(offset).unix_timestamp_nanos() / 1_000_000;
    i64::try_from(millis).map_err(|_| "到期时间超出支持范围".to_string())
}

fn local_datetime(timestamp: i64) -> Result<OffsetDateTime, String> {
    let utc =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp).saturating_mul(1_000_000))
            .map_err(|_| "到期时间无效".to_string())?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    Ok(utc.to_offset(offset))
}

fn parse_date(value: &str) -> Result<Date, String> {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .and_then(|part| part.parse::<i32>().ok())
        .ok_or_else(|| "到期日期无效".to_string())?;
    let month = parts
        .next()
        .and_then(|part| part.parse::<u8>().ok())
        .and_then(|value| Month::try_from(value).ok())
        .ok_or_else(|| "到期日期无效".to_string())?;
    let day = parts
        .next()
        .and_then(|part| part.parse::<u8>().ok())
        .ok_or_else(|| "到期日期无效".to_string())?;
    if parts.next().is_some() {
        return Err("到期日期无效".to_string());
    }
    Date::from_calendar_date(year, month, day).map_err(|_| "到期日期无效".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_default_evening_time_without_a_source() {
        let timestamp = timestamp_for_local_date("2026-06-10", None).unwrap();
        let local = local_datetime(timestamp).unwrap();

        assert_eq!(local_date_from_timestamp(timestamp).unwrap(), "2026-06-10");
        assert_eq!(local.hour(), 18);
        assert_eq!(local.minute(), 0);
    }

    #[test]
    fn preserves_source_hour_and_minute() {
        let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let source = (parse_date("2026-06-10")
            .unwrap()
            .with_hms(21, 30, 0)
            .unwrap()
            .assume_offset(offset)
            .unix_timestamp_nanos()
            / 1_000_000) as i64;
        let next = timestamp_for_local_date("2026-06-11", Some(source)).unwrap();
        let local = local_datetime(next).unwrap();

        assert_eq!(local_date_from_timestamp(next).unwrap(), "2026-06-11");
        assert_eq!(local.hour(), 21);
        assert_eq!(local.minute(), 30);
    }
}
