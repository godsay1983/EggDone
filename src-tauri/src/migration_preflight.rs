//! Read-only local migration gate. A settled snapshot is not backup or remote convergence proof.
use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;

const MAX_CLOCK: i64 = 9_007_199_254_740_991;
const RUNTIME_SQL: &str = "SELECT (SELECT value FROM app_metadata WHERE key='sync.target.epoch.v1') AS epoch,
last_result, last_success_at, dirty_domains, todos_dirty_version, notes_dirty_version, attachments_dirty_version
FROM sync_runtime_state WHERE id=1";
const STATE_SQL: &str = "SELECT 'rules' AS domain, revision, synced_revision, 0 AS generation, etag FROM recurrence_sync_state WHERE id=1
UNION ALL SELECT 'links', revision, synced_revision, 0, etag FROM task_note_link_sync_state WHERE id=1
UNION ALL SELECT domain, revision, synced_revision, generation, etag FROM task_checklist_sync_state
UNION ALL SELECT 'templates', revision, synced_revision, generation, etag FROM task_template_sync_state WHERE id=1
ORDER BY domain";
const INVENTORY_SQL: &str = "SELECT 'todos' AS name, COUNT(*) AS value FROM todos
UNION ALL SELECT 'groups', COUNT(*) FROM groups
UNION ALL SELECT 'notes', COUNT(*) FROM notes
UNION ALL SELECT 'attachments', COUNT(*) FROM note_attachments
UNION ALL SELECT 'rules', COUNT(*) FROM recurrence_rules
UNION ALL SELECT 'links', COUNT(*) FROM task_note_links
UNION ALL SELECT 'items', COUNT(*) FROM task_checklist_items
UNION ALL SELECT 'definitions', COUNT(*) FROM task_checklist_definitions
UNION ALL SELECT 'templates', COUNT(*) FROM task_templates
UNION ALL SELECT 'recurrence_instances', COUNT(*) FROM app_metadata WHERE key LIKE 'recurrence.instance.v1:%'
UNION ALL SELECT 'local_note_history', COUNT(*) FROM note_history
UNION ALL SELECT 'local_checklist_receipts', COUNT(*) FROM task_checklist_operations
UNION ALL SELECT 'local_template_receipts', COUNT(*) FROM task_template_operations
UNION ALL SELECT 'local_purge_terminals', COUNT(*) FROM lifecycle_terminals
UNION ALL SELECT 'local_purge_plans', COUNT(*) FROM purge_plans
UNION ALL SELECT 'pending_purge_targets', COUNT(*) FROM purge_targets WHERE outcome='pending' AND operation_uuid IN (SELECT operation_uuid FROM purge_plans WHERE state='running')
UNION ALL SELECT 'pending_purge_assets', COUNT(*) FROM purge_cleanup WHERE local_done=0 OR remote_done=0
UNION ALL SELECT 'archived_todos', COUNT(*) FROM todos WHERE archived_at IS NOT NULL AND deleted_at IS NULL
UNION ALL SELECT 'deleted_todos', COUNT(*) FROM todos WHERE deleted_at IS NOT NULL
UNION ALL SELECT 'deleted_notes', COUNT(*) FROM notes WHERE deleted_at IS NOT NULL
UNION ALL SELECT 'pending_uploads', COUNT(*) FROM note_attachments WHERE deleted_at IS NULL AND remote_uploaded=0
UNION ALL SELECT 'pending_asset_metadata', COUNT(*) FROM note_attachments WHERE deleted_at IS NULL AND transfer_state IN ('pending_upload','uploading','uploaded')
UNION ALL SELECT 'unuploaded_deleted_assets', COUNT(*) FROM note_attachments WHERE deleted_at IS NOT NULL AND remote_uploaded=0
UNION ALL SELECT 'original_paths_missing', COUNT(*) FROM note_attachments WHERE local_original_path IS NULL OR trim(local_original_path)=''
UNION ALL SELECT 'preview_paths_missing', COUNT(*) FROM note_attachments WHERE kind='image' AND (local_preview_path IS NULL OR trim(local_preview_path)='')
ORDER BY name";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainState {
    domain: String,
    revision: i64,
    synced_revision: i64,
    generation: i64,
    etag: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct InventoryCount {
    pub name: String,
    pub value: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalMigrationSnapshot {
    pub epoch: Option<String>,
    last_result: String,
    last_success_at: Option<i64>,
    dirty_domains: Vec<String>,
    versions: Vec<i64>,
    states: Vec<DomainState>,
    pub inventory: Vec<InventoryCount>,
}

impl LocalMigrationSnapshot {
    pub fn blockers(&self) -> Vec<String> {
        let mut result = Vec::new();
        if self
            .epoch
            .as_ref()
            .is_none_or(|s| s.is_empty() || s.starts_with("pending:"))
        {
            result.push("target_unconfirmed".into());
        }
        if self.last_result != "success" || self.last_success_at.is_none() {
            result.push("sync_not_successful".into());
        }
        for domain in &self.dirty_domains {
            result.push(format!("pending_{domain}"));
        }
        for state in &self.states {
            // Legacy synchronization deliberately skips an empty, never-published rules domain.
            let empty_rules = state.domain == "rules"
                && state.etag.is_none()
                && self
                    .inventory
                    .iter()
                    .any(|entry| entry.name == "rules" && entry.value == 0);
            if state.revision != state.synced_revision && !empty_rules {
                result.push(format!("pending_{}", state.domain));
            }
        }
        for name in [
            "pending_uploads",
            "pending_asset_metadata",
            "unuploaded_deleted_assets",
            "local_purge_terminals",
            "pending_purge_targets",
            "pending_purge_assets",
        ] {
            if self
                .inventory
                .iter()
                .any(|entry| entry.name == name && entry.value > 0)
            {
                result.push(name.into());
            }
        }
        result
    }

    // Never clear dirty flags or accept an old preview just because the new state is also clean.
    pub fn require_unchanged(&self, current: &Self) -> Result<(), String> {
        if !self.blockers().is_empty() || !current.blockers().is_empty() {
            return Err("MIGRATION_LOCAL_NOT_SETTLED".into());
        }
        if self != current {
            return Err("MIGRATION_LOCAL_CHANGED".into());
        }
        Ok(())
    }
}

fn invalid(_: rusqlite::Error) -> String {
    "MIGRATION_LOCAL_STATE_INVALID".into()
}
fn valid_clock(n: i64) -> bool {
    (0..=MAX_CLOCK).contains(&n)
}

/// Caller holds the normal sync exclusion and database mutex. No network or file I/O in this transaction.
pub(crate) fn read(connection: &mut Connection) -> Result<LocalMigrationSnapshot, String> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(invalid)?;
    let (epoch, last_result, last_success_at, dirty, versions) = tx
        .query_row(RUNTIME_SQL, [], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
                vec![
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ],
            ))
        })
        .map_err(invalid)?;
    let mut dirty_domains: Vec<String> =
        serde_json::from_str(&dirty).map_err(|_| "MIGRATION_LOCAL_STATE_INVALID")?;
    dirty_domains.sort();
    if dirty_domains
        .iter()
        .any(|s| !["todos", "notes", "attachments"].contains(&s.as_str()))
        || dirty_domains.windows(2).any(|p| p[0] == p[1])
        || versions.iter().any(|n| !valid_clock(*n))
        || last_success_at.is_some_and(|n| !valid_clock(n))
    {
        return Err("MIGRATION_LOCAL_STATE_INVALID".into());
    }
    let states = tx
        .prepare(STATE_SQL)
        .map_err(invalid)?
        .query_map([], |r| {
            Ok(DomainState {
                domain: r.get(0)?,
                revision: r.get(1)?,
                synced_revision: r.get(2)?,
                generation: r.get(3)?,
                etag: r.get(4)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if states.iter().map(|s| s.domain.as_str()).collect::<Vec<_>>()
        != ["definitions", "items", "links", "rules", "templates"]
        || states.iter().any(|s| {
            !valid_clock(s.revision)
                || !valid_clock(s.synced_revision)
                || s.synced_revision > s.revision
                || !valid_clock(s.generation)
        })
    {
        return Err("MIGRATION_LOCAL_STATE_INVALID".into());
    }
    let inventory = tx
        .prepare(INVENTORY_SQL)
        .map_err(invalid)?
        .query_map([], |r| {
            Ok(InventoryCount {
                name: r.get(0)?,
                value: r.get(1)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if inventory.iter().any(|v| !valid_clock(v.value)) {
        return Err("MIGRATION_LOCAL_STATE_INVALID".into());
    }
    tx.commit().map_err(invalid)?;
    Ok(LocalMigrationSnapshot {
        epoch,
        last_result,
        last_success_at,
        dirty_domains,
        versions,
        states,
        inventory,
    })
}

/// Acquires the same exclusion as normal synchronization. This is not a migration command.
pub(crate) fn inspect(
    db: &crate::db::Database,
    runtime: &crate::s3_sync::SyncRuntime,
) -> Result<LocalMigrationSnapshot, String> {
    let _guard = runtime.acquire().map_err(|_| "MIGRATION_SYNC_BUSY")?;
    let mut connection = db
        .connection
        .lock()
        .map_err(|_| "MIGRATION_LOCAL_STATE_INVALID")?;
    read(&mut connection)
}
