//! Durable fixed archive selections, executed in bounded transactions.
use crate::archive::{self, Action, Expected, Outcome};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ItemResult {
    pub uuid: String,
    pub result: Option<Outcome>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Job {
    pub operation_uuid: String,
    pub action: Action,
    pub scope: String,
    pub targets: Vec<Expected>,
    pub results: Vec<ItemResult>,
}
fn key(id: &str) -> String {
    format!("archive.batch.v1:{id}")
}
pub fn prepare(
    c: &mut Connection,
    operation: &str,
    action: Action,
    targets: &[Expected],
) -> Result<Job, String> {
    archive::id(operation)?;
    if action == Action::Reopen {
        return Err("ARCHIVE_INVALID_ACTION".into());
    }
    if targets.is_empty() {
        return Err("ARCHIVE_EMPTY_SELECTION".into());
    }
    // Bound a persisted selection explicitly; never silently truncate.
    if targets.len() > 10_000 {
        return Err("ARCHIVE_SELECTION_LIMIT".into());
    }
    let mut ids = HashSet::new();
    for t in targets {
        archive::validate_expected(t)?;
        if !ids.insert(&t.uuid) {
            return Err("ARCHIVE_DUPLICATE_TARGET".into());
        }
    }
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(archive::db)?;
    let scope = archive::scope(&tx)?;
    if archive::meta(&tx, &format!("archive.op.v1:{operation}"))?.is_some() {
        return Err("ARCHIVE_OPERATION_CONFLICT".into());
    }
    if targets.iter().any(|t| t.scope != scope) {
        return Err("ARCHIVE_SCOPE_CHANGED".into());
    }
    let result = if let Some(raw) = archive::meta(&tx, &key(operation))? {
        let job: Job = serde_json::from_str(&raw).map_err(|_| "ARCHIVE_INVALID_STATE")?;
        if job.action != action || job.targets != targets || job.scope != scope {
            return Err("ARCHIVE_OPERATION_CONFLICT".into());
        }
        job
    } else {
        let job = Job {
            operation_uuid: operation.into(),
            action,
            scope,
            targets: targets.to_vec(),
            results: Vec::new(),
        };
        archive::put(&tx, &key(operation), &archive::json(&job)?)?;
        job
    };
    tx.commit().map_err(archive::db)?;
    Ok(result)
}
pub fn get(c: &Connection, operation: &str) -> Result<Job, String> {
    archive::id(operation)?;
    let raw = archive::meta(c, &key(operation))?.ok_or("ARCHIVE_OPERATION_NOT_FOUND")?;
    serde_json::from_str(&raw).map_err(|_| "ARCHIVE_INVALID_STATE".into())
}
pub fn run(
    c: &mut Connection,
    operation: &str,
    limit: u32,
    now: i64,
    by: &str,
) -> Result<Job, String> {
    archive::id(operation)?;
    archive::version(now, by)?;
    if !(1..=50).contains(&limit) {
        return Err("ARCHIVE_INVALID_PAGE".into());
    }
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(archive::db)?;
    let mut job = get(&tx, operation)?;
    if archive::scope(&tx)? != job.scope {
        return Err("ARCHIVE_SCOPE_CHANGED".into());
    }
    let end = (job.results.len() + limit as usize).min(job.targets.len());
    for i in job.results.len()..end {
        let target = &job.targets[i];
        let result = match archive::mutate(&tx, target, job.action, now, by) {
            Ok(result) => ItemResult {
                uuid: target.uuid.clone(),
                result: Some(result),
                error: None,
            },
            Err(error)
                if [
                    "ARCHIVE_NOT_FOUND",
                    "ARCHIVE_NOT_ARCHIVED",
                    "ARCHIVE_CONFLICT",
                    "ARCHIVE_RULE_ACTIVE",
                ]
                .contains(&error.as_str()) =>
            {
                ItemResult {
                    uuid: target.uuid.clone(),
                    result: None,
                    error: Some(error),
                }
            }
            Err(error) => return Err(error),
        };
        job.results.push(result);
    }
    archive::put(&tx, &key(operation), &archive::json(&job)?)?;
    tx.commit().map_err(archive::db)?;
    Ok(job)
}
