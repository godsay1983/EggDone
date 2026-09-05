//! Atomic local preparation only. Serialized documents are not remote upload receipts.
use crate::{
    recurrence_links, recurrence_protocol, recurrence_store, recurrence_transaction, sync,
};
use recurrence_links::LinkPlan;
use recurrence_transaction::{AdvanceRequest, RecurrenceAction};
use rusqlite::{Connection, TransactionBehavior};

pub const MAX_RECONCILE_STEPS: usize = 128;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Debug)]
pub struct RecurrenceUploadSnapshot {
    pub todo_json: String,
    pub rules_json: String,
    pub todo_revision: i64,
    pub rule_revision: i64,
    pub advanced_count: usize,
}

#[derive(Debug)]
pub struct SnapshotPreparation {
    pub snapshot: Option<RecurrenceUploadSnapshot>,
    pub links: LinkPlan,
}

pub fn prepare_snapshot(
    connection: &mut Connection,
    now: i64,
) -> Result<SnapshotPreparation, String> {
    if !(0..MAX_SAFE).contains(&now) {
        return Err("INVALID_RECURRENCE_SNAPSHOT".into());
    }
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "RECURRENCE_DATABASE")?;
    let device_id: String = tx
        .query_row(
            "SELECT value FROM app_metadata WHERE key='device_id'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "RECURRENCE_DEVICE_MISSING")?;
    let mut advanced_count = 0;
    loop {
        let prepared = recurrence_links::prepare_links_in_transaction(&tx, now, &device_id)?;
        if !prepared.plan.can_prepare {
            // Dropping the transaction rolls back even successful earlier bindings/advances.
            return Ok(SnapshotPreparation {
                snapshot: None,
                links: prepared.plan,
            });
        }
        if let Some(link) = prepared.plan.links.iter().find(|l| l.state == "reconcile") {
            if advanced_count == MAX_RECONCILE_STEPS {
                return Err("RECURRENCE_RECONCILE_LIMIT".into());
            }
            let result = recurrence_transaction::advance_in_transaction(
                &tx,
                &AdvanceRequest {
                    rule_uuid: &link.rule_uuid,
                    current_todo_uuid: &link.todo_uuid,
                    action: RecurrenceAction::Reconcile,
                    now,
                    device_id: &device_id,
                    reminder_at: None,
                },
            )?;
            if result.is_none() {
                return Err("RECURRENCE_RECONCILE_STALLED".into());
            }
            advanced_count += 1;
            continue;
        }
        let todo = sync::build_document(&tx, now)?;
        sync::validate_document(&todo)?;
        let rules = recurrence_store::snapshot(&tx)?;
        let todo_revision: i64 = tx
            .query_row(
                "SELECT todos_dirty_version FROM sync_runtime_state WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "RECURRENCE_STATE_MISSING")?;
        if !(0..=MAX_SAFE).contains(&todo_revision) || !(0..=MAX_SAFE).contains(&rules.revision) {
            return Err("INVALID_RECURRENCE_SNAPSHOT".into());
        }
        let snapshot = RecurrenceUploadSnapshot {
            todo_json: serde_json::to_string(&todo).map_err(|_| "INVALID_RECURRENCE_SNAPSHOT")?,
            rules_json: recurrence_protocol::encode_document(&rules.document)?,
            todo_revision,
            rule_revision: rules.revision,
            advanced_count,
        };
        tx.commit().map_err(|_| "RECURRENCE_DATABASE")?;
        return Ok(SnapshotPreparation {
            snapshot: Some(snapshot),
            links: prepared.plan,
        });
    }
}

#[cfg(test)]
#[path = "recurrence_snapshot_tests.rs"]
pub(crate) mod tests;
