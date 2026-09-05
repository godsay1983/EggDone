//! Read-only advancement plan. Callers must read evidence and apply this plan in one transaction.
use crate::recurrence::{next_recurrence, recurrence_todo_uuid};
use crate::recurrence_protocol::{validate_rule, RecurrenceRule};
use crate::recurrence_time::recurrence_due_at;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionEvidence {
    pub uuid: String,
    pub completed: bool,
    pub deleted_at: Option<i64>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedOccurrence {
    pub uuid: String,
    pub date: String,
    pub index: u32,
    pub due_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurrenceAdvancePlan {
    pub rule: RecurrenceRule,
    pub next: Option<PlannedOccurrence>,
}

pub fn plan_recurrence_advance(
    rule: &RecurrenceRule,
    evidence: Option<&CompletionEvidence>,
    now: i64,
    device_id: &str,
) -> Result<Option<RecurrenceAdvancePlan>, String> {
    validate_rule(rule)?;
    if !(0..=9_007_199_254_740_991).contains(&now) {
        return Err("INVALID_RECURRENCE_PROGRESS".to_string());
    }
    let Some(evidence) = evidence else {
        return Ok(None);
    };
    if evidence
        .deleted_at
        .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
        || evidence
            .archived_at
            .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
    {
        return Err("INVALID_RECURRENCE_PROGRESS".to_string());
    }
    if rule.deleted_at.is_some()
        || rule.exhausted
        || evidence.uuid != rule.current_todo_uuid
        || (!evidence.completed && evidence.deleted_at.is_none())
    {
        return Ok(None);
    }
    let mut updated = rule.clone();
    updated.updated_at = now.max(rule.updated_at + 1);
    updated.updated_by = device_id.to_string();
    let next = match next_recurrence(&rule.schedule, &rule.current_date)? {
        None => {
            updated.exhausted = true;
            None
        }
        Some(occurrence) => {
            let uuid = recurrence_todo_uuid(&rule.uuid, &rule.schedule, &occurrence)?;
            let due_at = recurrence_due_at(
                &occurrence.date,
                rule.schedule.local_time_minutes,
                rule.timezone_id.as_deref(),
            )?;
            updated.current_todo_uuid = uuid.clone();
            updated.current_date = occurrence.date.clone();
            updated.generated_count = occurrence.index;
            Some(PlannedOccurrence {
                uuid,
                date: occurrence.date,
                index: occurrence.index,
                due_at,
            })
        }
    };
    validate_rule(&updated)?;
    Ok(Some(RecurrenceAdvancePlan {
        rule: updated,
        next,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence_protocol::{merge_documents, RecurrenceDocument};
    #[test]
    fn shared_recurrence_progress_fixtures() {
        let base: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/fixtures/recurrence-document-v1.json"
        ))
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/fixtures/recurrence-progress-v1.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let mut raw = base["base_rule"].clone();
            if let Some(patch) = case["rule_patch"].as_object() {
                for (key, value) in patch {
                    raw[key] = value.clone();
                }
            }
            if let Some(patch) = case["schedule_patch"].as_object() {
                for (key, value) in patch {
                    raw["schedule"][key] = value.clone();
                }
            }
            let rule: RecurrenceRule = serde_json::from_value(raw).unwrap();
            let before = rule.clone();
            let mut raw_evidence = serde_json::json!({"uuid": rule.current_todo_uuid,
                "completed":true,"deleted_at":null,"archived_at":null});
            if let Some(patch) = case["evidence_patch"].as_object() {
                for (key, value) in patch {
                    raw_evidence[key] = value.clone();
                }
            }
            let evidence: Option<CompletionEvidence> = if case["missing"] == true {
                None
            } else {
                Some(serde_json::from_value(raw_evidence).unwrap())
            };
            let now = case["now"].as_i64().unwrap_or(2000);
            let result = plan_recurrence_advance(
                &rule,
                evidence.as_ref(),
                now,
                case["device"].as_str().unwrap_or("device-b"),
            );
            assert_eq!(rule, before);
            if case["error"] == true {
                assert!(result.is_err(), "{}", case["id"]);
                continue;
            }
            if case["noop"] == true {
                assert!(result.unwrap().is_none(), "{}", case["id"]);
                continue;
            }
            let plan = result.unwrap().unwrap();
            let expected = &case["expected"];
            assert_eq!(
                plan.rule.exhausted,
                expected["exhausted"].as_bool().unwrap(),
                "{}",
                case["id"]
            );
            assert_eq!(
                plan.rule.generated_count,
                expected["index"].as_u64().unwrap() as u32
            );
            assert_eq!(
                plan.rule.updated_at,
                expected["updated_at"].as_i64().unwrap_or(now)
            );
            assert_eq!(
                plan.next.as_ref().map(|n| n.date.as_str()),
                expected["date"].as_str()
            );
            if let Some(next) = &plan.next {
                let utc = expected["utc"]
                    .as_str()
                    .map(|s| s.parse::<jiff::Timestamp>().unwrap().as_millisecond());
                assert_eq!(next.due_at, utc, "{}", case["id"]);
                assert_eq!(next.uuid, plan.rule.current_todo_uuid);
                assert_eq!(next.index, plan.rule.generated_count);
            }
            assert!(
                plan_recurrence_advance(&plan.rule, evidence.as_ref(), now + 1, "device-b")
                    .unwrap()
                    .is_none()
            );
            let concurrent =
                plan_recurrence_advance(&before, evidence.as_ref(), now + 1, "device-c")
                    .unwrap()
                    .unwrap();
            assert_eq!(concurrent.next, plan.next);
            let left = RecurrenceDocument {
                format_version: 1,
                rules: vec![plan.rule],
            };
            let right = RecurrenceDocument {
                format_version: 1,
                rules: vec![concurrent.rule],
            };
            assert_eq!(
                merge_documents(&left, &right).unwrap(),
                merge_documents(&right, &left).unwrap()
            );
        }
    }
}
