use crate::{
    daily_plan_store as plans,
    task_checklist_protocol::{valid_uuid, MAX_CLOCK},
    task_workflow_protocol::{self as protocol, Document, Event, Workflow},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct Snapshot {
    pub document: Document,
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub generation: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkflowWrite {
    pub operation_uuid: String,
    pub task_uuid: String,
    pub state: String,
    pub reason: String,
    #[serde(deserialize_with = "protocol::required_nullable")]
    pub review_date: Option<String>,
    pub date: String,
    pub remove_from_plan: bool,
    pub expected: String,
    #[serde(deserialize_with = "protocol::required_nullable")]
    pub expected_plan: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub task_uuid: String,
    pub reason: String,
    pub review_date: Option<String>,
    pub review_due: bool,
    pub clock: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WorkflowSnapshot {
    pub date: String,
    pub revision: String,
    pub entries: Vec<Entry>,
}
fn error(_: rusqlite::Error) -> String {
    "WORKFLOW_DATABASE".into()
}
pub fn read_in_transaction(tx: &Transaction<'_>) -> Result<Snapshot, String> {
    let mut s = tx.query_row("SELECT revision,synced_revision,etag,generation FROM task_workflow_sync_state WHERE id=1", [], |r| Ok(Snapshot {
        document: Document::default(), revision:r.get(0)?, synced_revision:r.get(1)?, etag:r.get(2)?, generation:r.get(3)?
    })).map_err(error)?;
    let mut q = tx
        .prepare("SELECT task_uuid,event_id FROM daily_plan_events ORDER BY task_uuid,event_id")
        .map_err(error)?;
    s.document.events = q
        .query_map([], |r| {
            Ok(Event {
                task_uuid: r.get(0)?,
                event_id: r.get(1)?,
            })
        })
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    let mut q = tx.prepare("SELECT task_uuid,state,reason,review_date,clock,writer,basis FROM task_workflow_states ORDER BY task_uuid").map_err(error)?;
    s.document.states = q
        .query_map([], |r| {
            Ok(Workflow {
                task_uuid: r.get(0)?,
                state: r.get(1)?,
                reason: r.get(2)?,
                review_date: r.get(3)?,
                clock: r.get(4)?,
                writer: r.get(5)?,
                basis: r.get(6)?,
            })
        })
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    protocol::encode(&s.document)?;
    Ok(s)
}
pub fn snapshot(db: &mut Connection) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = read_in_transaction(&tx)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
fn put(tx: &Transaction<'_>, s: &Workflow) -> Result<(), String> {
    tx.execute("INSERT INTO task_workflow_states(task_uuid,state,reason,review_date,clock,writer,basis) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(task_uuid) DO UPDATE SET state=excluded.state,reason=excluded.reason,review_date=excluded.review_date,clock=excluded.clock,writer=excluded.writer,basis=excluded.basis",params![s.task_uuid,s.state,s.reason,s.review_date,s.clock,s.writer,s.basis]).map_err(error)?;
    Ok(())
}
pub fn merge_in_transaction(tx: &Transaction<'_>, remote: &Document) -> Result<(), String> {
    protocol::encode(remote)?;
    let terminals = crate::lifecycle_sync::Index::read(tx).map_err(protocol::map_error)?;
    let mut local = read_in_transaction(tx)?.document;
    let mut remote = remote.clone();
    // Terminal dominance also excludes obsolete bodies from tie/conflict processing.
    local.states.retain(|s| !terminals.todo(&s.task_uuid));
    remote.states.retain(|s| !terminals.todo(&s.task_uuid));
    let merged = protocol::merge(&local, &remote)?;
    for e in merged.events {
        tx.execute(
            "INSERT OR IGNORE INTO daily_plan_events(task_uuid,event_id) VALUES(?1,?2)",
            params![e.task_uuid, e.event_id],
        )
        .map_err(error)?;
    }
    for s in merged.states {
        put(tx, &s)?;
    }
    tx.execute("DELETE FROM task_workflow_states WHERE task_uuid IN (SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    read_in_transaction(tx)?;
    plans::read_in_transaction(tx).map_err(protocol::map_error)?;
    Ok(())
}
pub fn restore(db: &mut Connection, remote: &Document) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    merge_in_transaction(&tx, remote)?;
    tx.commit().map_err(error)
}
fn project(tx: &Transaction<'_>, date: &str) -> Result<WorkflowSnapshot, String> {
    protocol::date(date)?;
    let s = read_in_transaction(tx)?;
    let parents = plans::parents(tx).map_err(protocol::map_error)?;
    let terminals = crate::purge::terminals(tx).map_err(protocol::map_error)?;
    let input = serde_json::to_vec(&(
        date,
        s.revision,
        s.generation,
        &s.document,
        &parents,
        &terminals,
    ))
    .map_err(|_| "WORKFLOW_INVALID")?;
    let revision = format!("{:x}", Sha256::digest(input));
    let mut entries = vec![];
    for w in &s.document.states {
        if w.state == "waiting"
            && parents.get(&w.task_uuid) == Some(&(false, None, None))
            && !terminals
                .iter()
                .any(|t| t.kind == "todo" && t.uuid == w.task_uuid)
            && w.basis == protocol::basis(&s.document, &w.task_uuid)
        {
            entries.push(Entry {
                task_uuid: w.task_uuid.clone(),
                reason: w.reason.clone(),
                review_date: w.review_date.clone(),
                review_due: w.review_date.as_deref().is_some_and(|d| d <= date),
                clock: w.clock,
            });
        }
    }
    entries.sort_by(|a, b| {
        (
            a.review_date.is_none(),
            &a.review_date,
            std::cmp::Reverse(a.clock),
            &a.task_uuid,
        )
            .cmp(&(
                b.review_date.is_none(),
                &b.review_date,
                std::cmp::Reverse(b.clock),
                &b.task_uuid,
            ))
    });
    Ok(WorkflowSnapshot {
        date: date.into(),
        revision,
        entries,
    })
}
pub fn list(db: &mut Connection, date: &str) -> Result<WorkflowSnapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = project(&tx, date)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
fn token(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn write(
    db: &mut Connection,
    r: &WorkflowWrite,
    now: i64,
    by: &str,
) -> Result<WorkflowSnapshot, String> {
    protocol::date(&r.date)?;
    protocol::fields(&r.state, &r.reason, r.review_date.as_deref())?;
    if !valid_uuid(&r.operation_uuid)
        || !valid_uuid(&r.task_uuid)
        || !token(&r.expected)
        || r.expected_plan.as_deref().is_some_and(|s| !token(s))
        || (r.remove_from_plan && (r.state != "waiting" || r.expected_plan.is_none()))
        || !crate::daily_plan_protocol::valid_writer(by)
        || !(0..=MAX_CLOCK).contains(&now)
    {
        return Err("WORKFLOW_INVALID".into());
    }
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(r).map_err(|_| "WORKFLOW_INVALID")?)
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT request_digest FROM task_workflow_operations WHERE operation_uuid=?1",
            [&r.operation_uuid],
            |q| q.get(0),
        )
        .optional()
        .map_err(error)?;
    if let Some(old) = old {
        if old != digest {
            return Err("WORKFLOW_CONFLICT".into());
        }
        let s = project(&tx, &r.date)?;
        tx.commit().map_err(error)?;
        return Ok(s);
    }
    if project(&tx, &r.date)?.revision != r.expected {
        return Err("WORKFLOW_CONFLICT".into());
    }
    if r.remove_from_plan
        && Some(
            plans::project(&tx, &r.date)
                .map_err(protocol::map_error)?
                .revision,
        ) != r.expected_plan
    {
        return Err("WORKFLOW_CONFLICT".into());
    }
    if plans::parents(&tx)
        .map_err(protocol::map_error)?
        .get(&r.task_uuid)
        != Some(&(false, None, None))
        || crate::lifecycle_sync::Index::read(&tx)
            .map_err(protocol::map_error)?
            .todo(&r.task_uuid)
    {
        return Err("WORKFLOW_UNAVAILABLE".into());
    }
    let s = read_in_transaction(&tx)?;
    let clock = now.max(
        s.document
            .states
            .iter()
            .map(|s| s.clock)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("WORKFLOW_LIMIT")?,
    );
    if clock > MAX_CLOCK {
        return Err("WORKFLOW_LIMIT".into());
    }
    put(
        &tx,
        &Workflow {
            task_uuid: r.task_uuid.clone(),
            state: r.state.clone(),
            reason: r.reason.clone(),
            review_date: r.review_date.clone(),
            clock,
            writer: by.into(),
            basis: protocol::basis(&s.document, &r.task_uuid),
        },
    )?;
    if r.remove_from_plan {
        let p = plans::read_in_transaction(&tx).map_err(protocol::map_error)?;
        let clock = now.max(
            p.document
                .plans
                .iter()
                .map(|p| p.clock)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("WORKFLOW_LIMIT")?,
        );
        if clock > MAX_CLOCK {
            return Err("WORKFLOW_LIMIT".into());
        }
        let position = p
            .document
            .plans
            .iter()
            .find(|p| p.task_uuid == r.task_uuid && p.plan_date == r.date)
            .map_or(0, |p| p.position);
        plans::put(
            &tx,
            &crate::daily_plan_protocol::Plan {
                task_uuid: r.task_uuid.clone(),
                plan_date: r.date.clone(),
                included: false,
                position,
                clock,
                writer: by.into(),
                basis: protocol::basis(&s.document, &r.task_uuid),
            },
        )
        .map_err(protocol::map_error)?;
        plans::read_in_transaction(&tx).map_err(protocol::map_error)?;
    }
    tx.execute(
        "INSERT INTO task_workflow_operations(operation_uuid,request_digest) VALUES(?1,?2)",
        params![r.operation_uuid, digest],
    )
    .map_err(error)?;
    let s = project(&tx, &r.date)?;
    tx.commit().map_err(error)?;
    Ok(s)
}

#[cfg(test)]
#[path = "task_workflow_store_tests.rs"]
mod tests;
