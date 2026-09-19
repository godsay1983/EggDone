use crate::{
    daily_plan_protocol::{self as protocol, Completion, Document, Event, Plan},
    task_checklist_protocol::{valid_uuid, MAX_CLOCK},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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
pub struct DailyPlanWrite {
    pub operation_uuid: String,
    pub task_uuid: String,
    pub plan_date: String,
    pub action: String,
    pub expected: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub task_uuid: String,
    pub plan_date: String,
    pub status: String,
    pub position: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DailyPlanSnapshot {
    pub date: String,
    pub revision: String,
    pub current: Vec<Entry>,
    pub previous: Vec<Entry>,
}
fn error(e: rusqlite::Error) -> String {
    format!("PLAN_DATABASE: {e}")
}

/// Remote parents carry generic edit clocks, not the original lifecycle version. Their
/// authoritative evidence arrives in the planning domain, so do not synthesize a second event.
pub fn without_lifecycle_events<T>(
    db: &Connection,
    apply: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    const KEY: &str = "daily.plan.remote-apply.v1";
    let previous: Option<String> = db
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,'1') ON CONFLICT(key) DO UPDATE SET value='1'",[KEY]).map_err(error)?;
    let result = apply();
    if let Some(previous) = previous {
        db.execute(
            "UPDATE app_metadata SET value=?1 WHERE key=?2",
            params![previous, KEY],
        )
        .map_err(error)?;
    } else {
        db.execute("DELETE FROM app_metadata WHERE key=?1", [KEY])
            .map_err(error)?;
    }
    result
}
pub fn read_in_transaction(tx: &Transaction<'_>) -> Result<Snapshot, String> {
    let mut s = tx
        .query_row(
            "SELECT revision,synced_revision,etag,generation FROM daily_plan_sync_state WHERE id=1",
            [],
            |r| {
                Ok(Snapshot {
                    document: Document::default(),
                    revision: r.get(0)?,
                    synced_revision: r.get(1)?,
                    etag: r.get(2)?,
                    generation: r.get(3)?,
                })
            },
        )
        .map_err(error)?;
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
    let mut q=tx.prepare("SELECT task_uuid,plan_date,included,position,clock,writer,basis FROM daily_plans ORDER BY task_uuid,plan_date").map_err(error)?;
    s.document.plans = q
        .query_map([], |r| {
            Ok(Plan {
                task_uuid: r.get(0)?,
                plan_date: r.get(1)?,
                included: r.get(2)?,
                position: r.get(3)?,
                clock: r.get(4)?,
                writer: r.get(5)?,
                basis: r.get(6)?,
            })
        })
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)?;
    let mut q=tx.prepare("SELECT task_uuid,plan_date,event_id,basis,position FROM daily_plan_completions ORDER BY task_uuid,plan_date,event_id").map_err(error)?;
    s.document.completions = q
        .query_map([], |r| {
            Ok(Completion {
                task_uuid: r.get(0)?,
                plan_date: r.get(1)?,
                event_id: r.get(2)?,
                basis: r.get(3)?,
                position: r.get(4)?,
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
pub(crate) fn put(tx: &Transaction<'_>, p: &Plan) -> Result<(), String> {
    tx.execute("INSERT INTO daily_plans(task_uuid,plan_date,included,position,clock,writer,basis) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(task_uuid,plan_date) DO UPDATE SET included=excluded.included,position=excluded.position,clock=excluded.clock,writer=excluded.writer,basis=excluded.basis",params![p.task_uuid,p.plan_date,p.included,p.position,p.clock,p.writer,p.basis]).map_err(error)?;
    Ok(())
}
pub fn merge_in_transaction(tx: &Transaction<'_>, remote: &Document) -> Result<(), String> {
    let mut merged = protocol::merge(&read_in_transaction(tx)?.document, remote)?;
    let terminals = crate::lifecycle_sync::Index::read(tx)?;
    merged.plans.retain(|p| !terminals.todo(&p.task_uuid));
    merged.completions.retain(|p| !terminals.todo(&p.task_uuid));
    for e in merged.events {
        tx.execute(
            "INSERT OR IGNORE INTO daily_plan_events(task_uuid,event_id) VALUES(?1,?2)",
            params![e.task_uuid, e.event_id],
        )
        .map_err(error)?;
    }
    for p in merged.plans {
        put(tx, &p)?;
    }
    for c in merged.completions {
        tx.execute("INSERT INTO daily_plan_completions(task_uuid,plan_date,event_id,basis,position) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(task_uuid,plan_date,event_id) DO UPDATE SET basis=excluded.basis,position=excluded.position",params![c.task_uuid,c.plan_date,c.event_id,c.basis,c.position]).map_err(error)?;
    }
    tx.execute("DELETE FROM daily_plans WHERE task_uuid IN (SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    tx.execute("DELETE FROM daily_plan_completions WHERE task_uuid IN (SELECT uuid FROM lifecycle_terminals WHERE kind='todo')",[]).map_err(error)?;
    read_in_transaction(tx)?;
    crate::task_workflow_store::read_in_transaction(tx)?;
    Ok(())
}
pub fn restore(db: &mut Connection, remote: &Document) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    merge_in_transaction(&tx, remote)?;
    tx.commit().map_err(error)
}
type Parents = BTreeMap<String, (bool, Option<i64>, Option<i64>)>;
pub(crate) fn parents(tx: &Transaction<'_>) -> Result<Parents, String> {
    let mut q = tx
        .prepare("SELECT uuid,completed,archived_at,deleted_at FROM todos ORDER BY uuid")
        .map_err(error)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ),
            ))
        })
        .map_err(error)?
        .collect::<Result<Parents, _>>()
        .map_err(error)?;
    Ok(rows)
}
pub(crate) fn project(tx: &Transaction<'_>, date: &str) -> Result<DailyPlanSnapshot, String> {
    let day = protocol::date(date)?;
    let previous = day.yesterday().map(|d| d.to_string()).unwrap_or_default();
    let s = read_in_transaction(tx)?;
    let parents = parents(tx)?;
    let terminals = crate::lifecycle_sync::Index::read(tx)?;
    let input = serde_json::to_vec(&(date, s.revision, s.generation, &parents))
        .map_err(|_| "PLAN_INVALID")?;
    let revision = format!("{:x}", Sha256::digest(input));
    let mut current = vec![];
    let mut yesterday = vec![];
    for p in &s.document.plans {
        if p.included
            && parents.get(&p.task_uuid) == Some(&(false, None, None))
            && !terminals.todo(&p.task_uuid)
            && p.basis == protocol::basis(&s.document, &p.task_uuid)
        {
            let entry = Entry {
                task_uuid: p.task_uuid.clone(),
                plan_date: p.plan_date.clone(),
                status: "planned".into(),
                position: p.position,
            };
            if p.plan_date == date {
                current.push(entry);
            } else if p.plan_date == previous {
                yesterday.push(entry);
            }
        }
    }
    for c in &s.document.completions {
        if c.plan_date == date
            && parents.get(&c.task_uuid) == Some(&(true, None, None))
            && !terminals.todo(&c.task_uuid)
            && protocol::completed_basis(c) == protocol::basis(&s.document, &c.task_uuid)
        {
            current.push(Entry {
                task_uuid: c.task_uuid.clone(),
                plan_date: c.plan_date.clone(),
                status: "completed".into(),
                position: c.position,
            });
        }
    }
    current.sort_by(|a, b| (a.position, &a.task_uuid).cmp(&(b.position, &b.task_uuid)));
    current.dedup_by(|a, b| a.task_uuid == b.task_uuid);
    yesterday.retain(|p| !current.iter().any(|c| c.task_uuid == p.task_uuid));
    yesterday.sort_by(|a, b| (a.position, &a.task_uuid).cmp(&(b.position, &b.task_uuid)));
    Ok(DailyPlanSnapshot {
        date: date.into(),
        revision,
        current,
        previous: yesterday,
    })
}
pub fn list(db: &mut Connection, date: &str) -> Result<DailyPlanSnapshot, String> {
    let tx = db.transaction().map_err(error)?;
    let s = project(&tx, date)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
pub fn write(
    db: &mut Connection,
    r: &DailyPlanWrite,
    now: i64,
    by: &str,
) -> Result<DailyPlanSnapshot, String> {
    protocol::date(&r.plan_date)?;
    if !valid_uuid(&r.task_uuid)
        || !valid_uuid(&r.operation_uuid)
        || r.expected.len() != 64
        || !r
            .expected
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || !protocol::valid_writer(by)
        || !(0..=MAX_CLOCK).contains(&now)
        || !matches!(r.action.as_str(), "add" | "remove" | "up" | "down")
    {
        return Err("PLAN_INVALID".into());
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let payload = serde_json::to_string(&(r, by)).map_err(|_| "PLAN_INVALID")?;
    let receipt: Option<String> = tx
        .query_row(
            "SELECT payload FROM daily_plan_operations WHERE operation_uuid=?1",
            [&r.operation_uuid],
            |q| q.get(0),
        )
        .optional()
        .map_err(error)?;
    if let Some(old) = receipt {
        if old != payload {
            return Err("PLAN_CONFLICT".into());
        }
        let s = project(&tx, &r.plan_date)?;
        tx.commit().map_err(error)?;
        return Ok(s);
    }
    let current = project(&tx, &r.plan_date)?;
    if current.revision != r.expected {
        return Err("PLAN_CONFLICT".into());
    }
    if parents(&tx)?.get(&r.task_uuid) != Some(&(false, None, None))
        || crate::lifecycle_sync::Index::read(&tx)?.todo(&r.task_uuid)
    {
        return Err("PLAN_UNAVAILABLE".into());
    }
    let s = read_in_transaction(&tx)?;
    let clock = now.max(
        s.document
            .plans
            .iter()
            .map(|p| p.clock)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("PLAN_LIMIT")?,
    );
    if clock > MAX_CLOCK {
        return Err("PLAN_LIMIT".into());
    }
    let planned: Vec<_> = current
        .current
        .iter()
        .filter(|p| p.status == "planned")
        .collect();
    let index = planned.iter().position(|p| p.task_uuid == r.task_uuid);
    let old = s
        .document
        .plans
        .iter()
        .find(|p| p.task_uuid == r.task_uuid && p.plan_date == r.plan_date);
    let make = |id: &str, position: i64, included: bool| Plan {
        task_uuid: id.into(),
        plan_date: r.plan_date.clone(),
        included,
        position,
        clock,
        writer: by.into(),
        basis: protocol::basis(&s.document, id),
    };
    match r.action.as_str() {
        "add" if index.is_none() => {
            let position = planned
                .iter()
                .map(|p| p.position)
                .max()
                .unwrap_or(-1)
                .checked_add(1)
                .ok_or("PLAN_LIMIT")?;
            if position > MAX_CLOCK {
                return Err("PLAN_LIMIT".into());
            }
            put(&tx, &make(&r.task_uuid, position, true))?;
        }
        "remove" => {
            put(
                &tx,
                &make(&r.task_uuid, old.map_or(0, |p| p.position), false),
            )?;
        }
        "up" | "down" => {
            let index = index.ok_or("PLAN_UNAVAILABLE")?;
            let other = if r.action == "up" {
                index.checked_sub(1)
            } else if index + 1 < planned.len() {
                Some(index + 1)
            } else {
                None
            };
            if let Some(other) = other {
                let mut order = planned;
                order.swap(index, other);
                for (position, p) in order.iter().enumerate() {
                    put(&tx, &make(&p.task_uuid, position as i64, true))?;
                }
            }
        }
        _ => (),
    }
    tx.execute(
        "INSERT INTO daily_plan_operations(operation_uuid,payload) VALUES(?1,?2)",
        params![r.operation_uuid, payload],
    )
    .map_err(error)?;
    let result = project(&tx, &r.plan_date)?;
    tx.commit().map_err(error)?;
    Ok(result)
}

#[cfg(test)]
#[path = "daily_plan_store_tests.rs"]
mod tests;
