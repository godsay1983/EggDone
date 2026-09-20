use super::*;
use serde_json::{json, Value};

pub(crate) const ACTIVE: &[u8] =
    include_bytes!("../../tests/fixtures/system-calendar-v1-active.json");
pub(crate) const WITHDRAWN: &[u8] =
    include_bytes!("../../tests/fixtures/system-calendar-v1-withdrawn.json");

fn value() -> Value {
    serde_json::from_slice(ACTIVE).unwrap()
}
fn accepts(value: &Value) -> bool {
    parse(&serde_json::to_vec(value).unwrap()).is_ok()
}

#[test]
fn system_calendar_shared_golden_and_derived_keys() {
    for bytes in [ACTIVE, WITHDRAWN] {
        let doc = parse(bytes).unwrap();
        assert_eq!(parse(&serde_json::to_vec(&doc).unwrap()).unwrap(), doc);
    }
    for (key, hash) in [
        (
            "eggdone/todos.json",
            "847efe17e35b350cbc846b56b909a7e428ea63a2253217e9f3be5f9681e2cff5",
        ),
        (
            "custom/\u{4e2d}\u{6587}.json",
            "b6d5004a35071fb41c42b4c65c0bb46ba2a3abd0bd4d48655f47ec0f0952356b",
        ),
    ] {
        assert_eq!(
            object_key(key),
            format!("eggdone-calendar/v1/{hash}/snapshot.json")
        );
    }
}

#[test]
fn system_calendar_strict_fields_types_ids_and_lengths() {
    let base = value();
    for field in base.as_object().unwrap().keys() {
        let mut changed = base.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(!accepts(&changed), "missing {field}");
    }
    for pointer in ["", "/coverage", "/calendars/0", "/occurrences/0"] {
        let mut changed = base.clone();
        changed.pointer_mut(pointer).unwrap()["extra"] = json!(1);
        assert!(!accepts(&changed));
        for field in base.pointer(pointer).unwrap().as_object().unwrap().keys() {
            let mut changed = base.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(!accepts(&changed), "{pointer}/{field}");
        }
    }
    for (pointer, invalid) in [
        ("/format_version", json!(2)),
        ("/revision", json!(0)),
        ("/revision", json!(9_007_199_254_740_992_i64)),
        ("/revision", json!(1.5)),
        ("/owner_id", json!("not-uuid")),
        ("/captured_at", json!(0)),
        ("/captured_at", json!(8_640_000_000_000_001_i64)),
        ("/occurrences/0/startTime", json!(9_007_199_254_740_992_i64)),
        ("/calendars/0/id", json!("A".repeat(64))),
        ("/occurrences/0/calendarId", json!("d".repeat(64))),
        ("/occurrences/0/isAllDay", json!("false")),
        ("/occurrences/0/title", json!("x".repeat(5001))),
        ("/occurrences/0/location", json!("\u{1f600}".repeat(2501))),
        ("/occurrences/0/timeZone", json!("z".repeat(129))),
        ("/occurrences/0/timeZone", json!("")),
        ("/source_timezone", json!("z".repeat(129))),
        ("/source_timezone", json!("bad/timezone")),
    ] {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = invalid;
        assert!(!accepts(&changed), "{pointer}");
    }
    let mut changed = base.clone();
    changed["occurrences"][0]["title"] = json!("\u{1f600}".repeat(2500));
    assert!(accepts(&changed));
    for field in ["calendars", "occurrences"] {
        let mut changed = base.clone();
        let duplicate = changed[field][0].clone();
        changed[field].as_array_mut().unwrap().push(duplicate);
        assert!(!accepts(&changed));
    }
    let duplicate = String::from_utf8(ACTIVE.to_vec()).unwrap().replacen(
        "\"revision\": 1",
        "\"revision\": 1, \"revision\": 2",
        1,
    );
    assert!(parse(duplicate.as_bytes()).is_err());
    assert!(parse(b"\xff").is_err());
    assert_eq!(
        parse(&vec![b' '; MAX_BYTES + 1]).unwrap_err(),
        "CALENDAR_DOCUMENT_TOO_LARGE"
    );
}

#[test]
fn system_calendar_dates_coverage_zero_duration_and_withdrawal() {
    let base = value();
    for (pointer, invalid) in [
        ("/coverage/start", json!("2026-02-30")),
        ("/coverage/end", json!("2027-03-21")),
        ("/coverage/start_time", json!(1787241600001_i64)),
        ("/occurrences/0/startDate", json!("2026-09-20")),
        ("/occurrences/0/endTime", json!(0)),
        ("/occurrences/1/endDateExclusive", json!("2026-09-20")),
        ("/occurrences/1/startTime", json!(1789862400001_i64)),
    ] {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = invalid;
        assert!(!accepts(&changed), "{pointer}");
    }
    let mut changed = base.clone();
    changed["occurrences"][0]["startTime"] = changed["coverage"]["start_time"].clone();
    changed["occurrences"][0]["endTime"] = changed["coverage"]["start_time"].clone();
    assert!(accepts(&changed));
    changed["occurrences"][0]["startTime"] = changed["coverage"]["end_time"].clone();
    changed["occurrences"][0]["endTime"] = changed["coverage"]["end_time"].clone();
    assert!(!accepts(&changed));
    let mut withdrawn: Value = serde_json::from_slice(WITHDRAWN).unwrap();
    withdrawn["calendars"] = base["calendars"].clone();
    assert!(!accepts(&withdrawn));
}

#[test]
fn system_calendar_accepts_dst_coverage_and_enforces_count() {
    let mut doc = value();
    doc["source_timezone"] = json!("America/New_York");
    for (date_key, time_key) in [("start", "start_time"), ("end", "end_time")] {
        let date: jiff::civil::Date = doc["coverage"][date_key].as_str().unwrap().parse().unwrap();
        let zoned = date.at(0, 0, 0, 0).in_tz("America/New_York").unwrap();
        doc["coverage"][time_key] = json!(zoned.timestamp().as_millisecond());
    }
    assert!(accepts(&doc));
    let original = doc["occurrences"][0].clone();
    let items: Vec<_> = (0..10_000)
        .map(|i| {
            let mut item = original.clone();
            item["id"] = json!(format!("{i:064x}"));
            item
        })
        .collect();
    doc["occurrences"] = json!(items);
    assert!(accepts(&doc));
    doc["occurrences"].as_array_mut().unwrap().push(original);
    assert!(!accepts(&doc));
}
