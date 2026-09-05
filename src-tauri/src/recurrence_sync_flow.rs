//! Ordered workflow only. Production entry points must supply a target-scoped I/O adapter.
use crate::recurrence_snapshot::{RecurrenceUploadSnapshot, SnapshotPreparation};
use crate::recurrence_transport::RuleUploadOutcome;
use std::future::Future;

pub const MAX_SYNC_ATTEMPTS: usize = 2;

// Each remote value must retain the transport's download owner and conditional-write token.
// Adapters never hold a database transaction across network awaits.
pub trait RecurrenceSyncPort {
    type Remote;
    fn target_is_current(&mut self) -> impl Future<Output = Result<bool, String>>;
    fn download(&mut self) -> impl Future<Output = Result<Self::Remote, String>>;
    fn prepare(
        &mut self,
        remote: &Self::Remote,
    ) -> impl Future<Output = Result<SnapshotPreparation, String>>;
    fn upload_todos(
        &mut self,
        remote: &Self::Remote,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> impl Future<Output = Result<bool, String>>;
    fn acknowledge_todos(&mut self, revision: i64) -> impl Future<Output = Result<bool, String>>;
    fn upload_rules(
        &mut self,
        remote: &Self::Remote,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> impl Future<Output = Result<RuleUploadOutcome, String>>;
    fn acknowledge_rules(
        &mut self,
        revision: i64,
        etag: &str,
    ) -> impl Future<Output = Result<bool, String>>;
    fn snapshot_is_current(
        &mut self,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> impl Future<Output = Result<bool, String>>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecurrenceSyncResult {
    pub rule_etag: String,
    pub attempts: usize,
    pub todo_acknowledged: bool,
    pub rules_acknowledged: bool,
    pub local_changes_pending: bool,
    pub todo_revision: i64,
    pub rule_revision: i64,
}

async fn guard(port: &mut impl RecurrenceSyncPort) -> Result<(), String> {
    if port.target_is_current().await? {
        Ok(())
    } else {
        Err("RECURRENCE_CONFIG_CHANGED".into())
    }
}

pub async fn run(port: &mut impl RecurrenceSyncPort) -> Result<RecurrenceSyncResult, String> {
    for attempt in 1..=MAX_SYNC_ATTEMPTS {
        guard(port).await?;
        let remote = port.download().await?;
        guard(port).await?;
        let prepared = port.prepare(&remote).await?;
        guard(port).await?;
        let Some(snapshot) = prepared.snapshot else {
            if attempt == MAX_SYNC_ATTEMPTS {
                return Err("RECURRENCE_LINKS_BLOCKED".into());
            }
            continue;
        };
        let todo_uploaded = port.upload_todos(&remote, &snapshot).await?;
        guard(port).await?;
        if !todo_uploaded {
            if attempt == MAX_SYNC_ATTEMPTS {
                return Err("RECURRENCE_SYNC_CONFLICT".into());
            }
            continue;
        }
        // A rule failure does not invalidate a confirmed Todo upload for the current target.
        let todo_acknowledged = port.acknowledge_todos(snapshot.todo_revision).await?;
        guard(port).await?;
        let uploaded = port.upload_rules(&remote, &snapshot).await?;
        guard(port).await?;
        let RuleUploadOutcome::Uploaded { etag } = uploaded else {
            if attempt == MAX_SYNC_ATTEMPTS {
                return Err("RECURRENCE_SYNC_CONFLICT".into());
            }
            continue;
        };
        let rules_acknowledged = port
            .acknowledge_rules(snapshot.rule_revision, &etag)
            .await?;
        guard(port).await?;
        let current = port.snapshot_is_current(&snapshot).await?;
        guard(port).await?;
        return Ok(RecurrenceSyncResult {
            rule_etag: etag,
            attempts: attempt,
            todo_acknowledged,
            rules_acknowledged,
            local_changes_pending: !current || !todo_acknowledged || !rules_acknowledged,
            todo_revision: snapshot.todo_revision,
            rule_revision: snapshot.rule_revision,
        });
    }
    Err("RECURRENCE_SYNC_CONFLICT".into())
}

#[cfg(test)]
#[path = "recurrence_sync_flow_tests.rs"]
mod tests;
