use crate::task_checklist_protocol::{valid_uuid, MAX_CLOCK};
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_ROWS: usize = 20_000;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EVENTS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub task_uuid: String,
    pub event_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub task_uuid: String,
    pub plan_date: String,
    pub included: bool,
    pub position: i64,
    pub clock: i64,
    pub writer: String,
    pub basis: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub task_uuid: String,
    pub plan_date: String,
    pub event_id: String,
    pub basis: String,
    pub position: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format_version: u32,
    pub events: Vec<Event>,
    pub plans: Vec<Plan>,
    pub completions: Vec<Completion>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            format_version: 1,
            events: vec![],
            plans: vec![],
            completions: vec![],
        }
    }
}
pub fn is_empty(d: &Document) -> bool {
    d.events.is_empty() && d.plans.is_empty() && d.completions.is_empty()
}
fn invalid() -> String {
    "PLAN_INVALID".into()
}
pub fn date(s: &str) -> Result<Date, String> {
    let d = s.parse::<Date>().map_err(|_| invalid())?;
    if !(1..=9999).contains(&d.year()) || d.to_string() != s {
        return Err(invalid());
    }
    Ok(d)
}
pub fn valid_writer(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn stamp(s: &str) -> bool {
    s.parse::<i64>()
        .is_ok_and(|v| (0..=MAX_CLOCK).contains(&v) && v.to_string() == s)
}
pub fn valid_event(s: &str) -> bool {
    let p: Vec<_> = s.split(':').collect();
    p.len() == 5
        && stamp(p[0])
        && !p[1].is_empty()
        && p[1].len() <= 256
        && p[1].len() % 2 == 0
        && p[1]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && matches!(p[2], "0" | "1")
        && (p[3] == "-" || stamp(p[3]))
        && (p[4] == "-" || stamp(p[4]))
}
pub fn valid_basis(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let ids: Vec<_> = s.split('|').collect();
    ids.len() <= MAX_EVENTS
        && ids.iter().all(|id| valid_event(id))
        && ids.windows(2).all(|v| v[0] < v[1])
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
pub fn completed_basis(c: &Completion) -> String {
    let mut ids: BTreeSet<_> = c.basis.split('|').filter(|v| !v.is_empty()).collect();
    ids.insert(&c.event_id);
    ids.into_iter().collect::<Vec<_>>().join("|")
}
pub fn validate(d: &Document) -> Result<(), String> {
    if d.format_version != 1
        || [d.events.len(), d.plans.len(), d.completions.len()]
            .iter()
            .any(|n| *n > MAX_ROWS)
    {
        return Err(invalid());
    }
    let mut events = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for e in &d.events {
        let count = counts.entry(&e.task_uuid).or_insert(0);
        *count += 1;
        if !valid_uuid(&e.task_uuid)
            || !valid_event(&e.event_id)
            || !events.insert((&e.task_uuid, &e.event_id))
            || *count > MAX_EVENTS
        {
            return Err(invalid());
        }
    }
    let mut plans = BTreeSet::new();
    for p in &d.plans {
        date(&p.plan_date)?;
        if !valid_uuid(&p.task_uuid)
            || !valid_writer(&p.writer)
            || !(0..=MAX_CLOCK).contains(&p.clock)
            || !(0..=MAX_CLOCK).contains(&p.position)
            || !valid_basis(&p.basis)
            || !plans.insert((&p.task_uuid, &p.plan_date))
        {
            return Err(invalid());
        }
    }
    let mut completions = BTreeSet::new();
    for c in &d.completions {
        date(&c.plan_date)?;
        if !valid_uuid(&c.task_uuid)
            || !valid_event(&c.event_id)
            || !valid_basis(&c.basis)
            || !(0..=MAX_CLOCK).contains(&c.position)
            || c.basis.split('|').any(|e| e == c.event_id)
            || !completions.insert((&c.task_uuid, &c.plan_date, &c.event_id))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub fn encode(d: &Document) -> Result<String, String> {
    validate(d)?;
    let mut d = d.clone();
    d.events.sort();
    d.plans
        .sort_by(|a, b| (&a.task_uuid, &a.plan_date).cmp(&(&b.task_uuid, &b.plan_date)));
    d.completions.sort_by(|a, b| {
        (&a.task_uuid, &a.plan_date, &a.event_id).cmp(&(&b.task_uuid, &b.plan_date, &b.event_id))
    });
    let raw = serde_json::to_string(&d).map_err(|_| invalid())?;
    if raw.len() > MAX_BYTES {
        return Err("PLAN_LIMIT".into());
    }
    Ok(raw)
}
pub fn parse(raw: &str) -> Result<Document, String> {
    if raw.len() > MAX_BYTES {
        return Err("PLAN_LIMIT".into());
    }
    // JSON integer-valued numbers have the same semantics on Rust and ArkTS.
    let mut value: serde_json::Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    fn integers(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Number(n) if n.is_f64() => {
                if let Some(f) = n.as_f64() {
                    if f.is_finite() && f.fract() == 0.0 && f.abs() <= MAX_CLOCK as f64 {
                        *n = serde_json::Number::from(f as i64);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(integers),
            serde_json::Value::Object(o) => o.values_mut().for_each(integers),
            _ => (),
        }
    }
    integers(&mut value);
    let d: Document = serde_json::from_value(value).map_err(|_| invalid())?;
    validate(&d)?;
    Ok(d)
}
pub fn merge(a: &Document, b: &Document) -> Result<Document, String> {
    validate(a)?;
    validate(b)?;
    let mut events = BTreeSet::new();
    let mut plans: BTreeMap<(String, String), Plan> = BTreeMap::new();
    let mut completions: BTreeMap<(String, String, String), Completion> = BTreeMap::new();
    for d in [a, b] {
        events.extend(d.events.iter().cloned());
        for p in &d.plans {
            let key = (p.task_uuid.clone(), p.plan_date.clone());
            let rank = |p: &Plan| {
                (
                    p.clock,
                    p.writer.clone(),
                    p.included,
                    p.position,
                    p.basis.clone(),
                )
            };
            if plans.get(&key).is_none_or(|old| rank(p) > rank(old)) {
                plans.insert(key, p.clone());
            }
        }
        for c in &d.completions {
            let key = (c.task_uuid.clone(), c.plan_date.clone(), c.event_id.clone());
            if completions
                .get(&key)
                .is_none_or(|old| (&c.basis, c.position) > (&old.basis, old.position))
            {
                completions.insert(key, c.clone());
            }
        }
    }
    let d = Document {
        format_version: 1,
        events: events.into_iter().collect(),
        plans: plans.into_values().collect(),
        completions: completions.into_values().collect(),
    };
    encode(&d)?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_harmony_fixtures() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/fixtures/daily-planning-v1.json"))
                .unwrap();
        for case in fixtures["valid"].as_array().unwrap() {
            let parsed = parse(&case["document"].to_string()).unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&encode(&parsed).unwrap()).unwrap(),
                case["document"],
                "{}",
                case["id"]
            );
        }
        for case in fixtures["invalid"].as_array().unwrap() {
            assert!(
                parse(&case["document"].to_string()).is_err(),
                "{}",
                case["id"]
            );
        }
        for case in fixtures["merges"].as_array().unwrap() {
            let a = parse(&case["left"].to_string()).unwrap();
            let b = parse(&case["right"].to_string()).unwrap();
            let expected = parse(&case["expected"].to_string()).unwrap();
            assert_eq!(merge(&a, &b).unwrap(), expected, "{}", case["id"]);
            assert_eq!(merge(&b, &a).unwrap(), expected, "{}", case["id"]);
        }
    }
    #[test]
    fn dates_and_evidence_are_strict() {
        assert!(date("2024-02-29").is_ok());
        assert!(date("2025-02-29").is_err());
        assert!(date("2026-2-01").is_err());
        assert!(date("0000-01-01").is_err());
        assert!(valid_basis("1:61:0:-:-|2:61:1:-:-"));
        assert!(!valid_basis("2:61:1:-:-|1:61:0:-:-"));
        assert!(!valid_event("01:61:0:-:-"));
        assert!(
            parse(r#"{"format_version":1,"events":[],"plans":[],"completions":[],"extra":1}"#)
                .is_err()
        );
    }
    #[test]
    fn merge_is_commutative_and_idempotent() {
        let p = Plan {
            task_uuid: "11111111-1111-4111-8111-111111111111".into(),
            plan_date: "2026-09-19".into(),
            included: true,
            position: 0,
            clock: 1,
            writer: "a".into(),
            basis: "".into(),
        };
        let a = Document {
            plans: vec![p.clone()],
            ..Default::default()
        };
        let b = Document {
            plans: vec![Plan {
                included: false,
                clock: 2,
                ..p
            }],
            ..Default::default()
        };
        assert_eq!(merge(&a, &b).unwrap(), merge(&b, &a).unwrap());
        assert_eq!(merge(&b, &b).unwrap(), b);
        assert!(!merge(&a, &b).unwrap().plans[0].included);
    }
}
