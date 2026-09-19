use crate::{
    daily_plan_protocol as plans,
    task_checklist_protocol::{valid_uuid, MAX_CLOCK},
};
pub use plans::Event;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub task_uuid: String,
    pub state: String,
    pub reason: String,
    #[serde(deserialize_with = "required_nullable")]
    pub review_date: Option<String>,
    pub clock: i64,
    pub writer: String,
    pub basis: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: u32,
    pub events: Vec<Event>,
    pub states: Vec<Workflow>,
}
pub fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
impl Default for Document {
    fn default() -> Self {
        Self {
            format_version: 1,
            events: vec![],
            states: vec![],
        }
    }
}
pub fn is_empty(d: &Document) -> bool {
    d.events.is_empty() && d.states.is_empty()
}
pub fn map_error(e: String) -> String {
    if e.contains("LIMIT") {
        "WORKFLOW_LIMIT".into()
    } else if e.contains("CONFLICT") {
        "WORKFLOW_CONFLICT".into()
    } else if e.contains("INVALID") {
        "WORKFLOW_INVALID".into()
    } else {
        "WORKFLOW_DATABASE".into()
    }
}
pub fn date(s: &str) -> Result<(), String> {
    plans::date(s).map(|_| ()).map_err(map_error)
}
pub fn fields(state: &str, reason: &str, review: Option<&str>) -> Result<(), String> {
    if !matches!(state, "ready" | "waiting")
        || reason.encode_utf16().count() > 200
        || (state == "ready" && (!reason.is_empty() || review.is_some()))
    {
        return Err("WORKFLOW_INVALID".into());
    }
    if let Some(d) = review {
        date(d)?;
    }
    Ok(())
}
pub fn validate(d: &Document) -> Result<(), String> {
    if d.format_version != 1 {
        return Err("WORKFLOW_INVALID".into());
    }
    if d.events.len() > plans::MAX_ROWS || d.states.len() > plans::MAX_ROWS {
        return Err("WORKFLOW_LIMIT".into());
    }
    plans::validate(&plans::Document {
        events: d.events.clone(),
        ..Default::default()
    })
    .map_err(map_error)?;
    let mut ids = BTreeSet::new();
    for s in &d.states {
        fields(&s.state, &s.reason, s.review_date.as_deref())?;
        if !valid_uuid(&s.task_uuid)
            || !ids.insert(&s.task_uuid)
            || !plans::valid_writer(&s.writer)
            || !(0..=MAX_CLOCK).contains(&s.clock)
            || !plans::valid_basis(&s.basis)
        {
            return Err("WORKFLOW_INVALID".into());
        }
    }
    Ok(())
}
pub fn encode(d: &Document) -> Result<String, String> {
    validate(d)?;
    let mut d = d.clone();
    d.events.sort();
    d.states.sort_by(|a, b| a.task_uuid.cmp(&b.task_uuid));
    let raw = serde_json::to_string(&d).map_err(|_| "WORKFLOW_INVALID")?;
    if raw.len() > plans::MAX_BYTES {
        return Err("WORKFLOW_LIMIT".into());
    }
    Ok(raw)
}
pub fn parse(raw: &str) -> Result<Document, String> {
    if raw.len() > plans::MAX_BYTES {
        return Err("WORKFLOW_LIMIT".into());
    }
    let mut v: serde_json::Value = serde_json::from_str(raw).map_err(|_| "WORKFLOW_INVALID")?;
    fn integers(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Number(n) if n.is_f64() => {
                if let Some(f) = n.as_f64() {
                    if f.is_finite() && f.fract() == 0.0 && f.abs() <= MAX_CLOCK as f64 {
                        *n = (f as i64).into();
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(integers),
            serde_json::Value::Object(o) => o.values_mut().for_each(integers),
            _ => (),
        }
    }
    integers(&mut v);
    let d = serde_json::from_value(v).map_err(|_| "WORKFLOW_INVALID")?;
    validate(&d)?;
    Ok(d)
}
fn rank(a: &Workflow, b: &Workflow) -> Ordering {
    (
        a.clock,
        &a.writer,
        a.state == "ready",
        a.review_date.as_deref().unwrap_or(""),
    )
        .cmp(&(
            b.clock,
            &b.writer,
            b.state == "ready",
            b.review_date.as_deref().unwrap_or(""),
        ))
        .then_with(|| a.reason.encode_utf16().cmp(b.reason.encode_utf16()))
}
pub fn merge(a: &Document, b: &Document) -> Result<Document, String> {
    validate(a)?;
    validate(b)?;
    let mut states: BTreeMap<String, Workflow> = BTreeMap::new();
    let mut events = BTreeSet::new();
    for d in [a, b] {
        events.extend(d.events.iter().cloned());
        for s in &d.states {
            if let Some(old) = states.get(&s.task_uuid) {
                match rank(s, old) {
                    Ordering::Less => continue,
                    Ordering::Equal if s != old => return Err("WORKFLOW_CONFLICT".into()),
                    Ordering::Equal => continue,
                    Ordering::Greater => (),
                }
            }
            states.insert(s.task_uuid.clone(), s.clone());
        }
    }
    let d = Document {
        format_version: 1,
        events: events.into_iter().collect(),
        states: states.into_values().collect(),
    };
    encode(&d)?;
    Ok(d)
}
pub fn basis(d: &Document, task: &str) -> String {
    d.events
        .iter()
        .filter(|e| e.task_uuid == task)
        .map(|e| e.event_id.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_shared_cross_client_fixtures() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/fixtures/task-workflow-v1.json"))
                .unwrap();
        for case in fixtures["valid"].as_array().unwrap() {
            let parsed = parse(&case.to_string()).unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&encode(&parsed).unwrap()).unwrap(),
                *case
            );
        }
        for case in fixtures["invalid"].as_array().unwrap() {
            assert!(parse(&case.to_string()).is_err(), "{case}");
        }
        for raw in fixtures["invalid_raw"].as_array().unwrap() {
            assert_eq!(
                parse(raw.as_str().unwrap()).unwrap_err(),
                "WORKFLOW_INVALID"
            );
        }
        for case in fixtures["merges"].as_array().unwrap() {
            let a = parse(&case["left"].to_string()).unwrap();
            let b = parse(&case["right"].to_string()).unwrap();
            let expected = parse(&case["expected"].to_string()).unwrap();
            assert_eq!(merge(&a, &b).unwrap(), expected, "{}", case["name"]);
            assert_eq!(merge(&b, &a).unwrap(), expected, "{}", case["name"]);
        }
    }
    fn document() -> Document {
        Document {
            states: vec![Workflow {
                task_uuid: "123e4567-e89b-42d3-a456-426614174000".into(),
                state: "waiting".into(),
                reason: "".into(),
                review_date: None,
                clock: 1,
                writer: "a".into(),
                basis: "".into(),
            }],
            ..Default::default()
        }
    }
    #[test]
    fn workflow_validation_strict_fields_dates_utf16_and_limits() {
        let mut d = document();
        d.states[0].reason = "\u{1f600}".repeat(100);
        assert!(encode(&d).is_ok());
        d.states[0].reason.push('a');
        assert!(encode(&d).is_err());
        d.states[0].reason.clear();
        for bad in ["2025-02-29", "2026-2-01", "0000-01-01", "10000-01-01"] {
            d.states[0].review_date = Some(bad.into());
            assert!(encode(&d).is_err());
        }
        d.states[0].review_date = Some("2024-02-29".into());
        assert!(encode(&d).is_ok());
        let mut v = serde_json::to_value(&d).unwrap();
        v["states"][0]["unknown"] = true.into();
        assert!(parse(&v.to_string()).is_err());
        d.states.push(d.states[0].clone());
        assert!(encode(&d).is_err());
        assert_eq!(
            parse(&" ".repeat(plans::MAX_BYTES + 1)).unwrap_err(),
            "WORKFLOW_LIMIT"
        );
        let mut d = document();
        d.events = (0..=plans::MAX_EVENTS)
            .map(|n| Event {
                task_uuid: d.states[0].task_uuid.clone(),
                event_id: format!("{n}:61:0:-:-"),
            })
            .collect();
        assert!(encode(&d).is_err());
        let numeric = encode(&document())
            .unwrap()
            .replace("\"clock\":1", "\"clock\":1.0");
        assert!(parse(&numeric).is_ok());
    }
    #[test]
    fn workflow_merge_laws_ready_wins_and_reason_uses_utf16() {
        let a = document();
        let mut b = a.clone();
        b.states[0].reason = "\u{e000}".into();
        let mut c = a.clone();
        c.states[0].reason = "\u{10000}".into();
        assert_eq!(merge(&b, &c).unwrap(), b);
        assert_eq!(merge(&a, &b).unwrap(), merge(&b, &a).unwrap());
        assert_eq!(merge(&a, &a).unwrap(), a);
        assert_eq!(
            merge(&merge(&a, &b).unwrap(), &c).unwrap(),
            merge(&a, &merge(&b, &c).unwrap()).unwrap()
        );
        c.states[0].state = "ready".into();
        c.states[0].reason.clear();
        assert_eq!(merge(&b, &c).unwrap(), c);
        let mut unknown = a.clone();
        unknown.states[0].basis = "1:61:0:-:-".into();
        assert_eq!(merge(&a, &unknown).unwrap_err(), "WORKFLOW_CONFLICT");
        assert_eq!(merge(&unknown, &a).unwrap_err(), "WORKFLOW_CONFLICT");
    }
}
