use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub kind: String,
    pub uuid: String,
    pub operation_uuid: String,
    pub purged_at: i64,
}

pub fn terminals(connection: &Connection) -> Result<Vec<Terminal>, String> {
    let mut query = connection
        .prepare(
            "SELECT kind,uuid,operation_uuid,purged_at FROM lifecycle_terminals ORDER BY kind,uuid",
        )
        .map_err(db_error)?;
    let result = query
        .query_map([], |r| {
            Ok(Terminal {
                kind: r.get(0)?,
                uuid: r.get(1)?,
                operation_uuid: r.get(2)?,
                purged_at: r.get(3)?,
            })
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    validate_terminals(&result)?;
    Ok(result)
}
pub fn validate_terminals(records: &[Terminal]) -> Result<(), String> {
    if records.len() > 100_000 {
        return Err("PURGE_LEDGER_LIMIT".into());
    }
    let mut seen = BTreeSet::new();
    for record in records {
        validate(&Target {
            kind: record.kind.clone(),
            uuid: record.uuid.clone(),
        })?;
        id(&record.operation_uuid)?;
        if !(0..=MAX_SAFE).contains(&record.purged_at) || !seen.insert((&record.kind, &record.uuid))
        {
            return Err("PURGE_LEDGER_INVALID".into());
        }
    }
    Ok(())
}
pub fn restore_terminals(connection: &Connection, records: &[Terminal]) -> Result<(), String> {
    validate_terminals(records)?;
    for record in records {
        let exists: bool = connection
            .query_row(
                if record.kind == "todo" {
                    "SELECT EXISTS(SELECT 1 FROM todos WHERE uuid=?1)"
                } else {
                    "SELECT EXISTS(SELECT 1 FROM notes WHERE uuid=?1)"
                },
                [&record.uuid],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if exists {
            return Err("PURGE_IMPORT_CONFLICT".into());
        }
        erase(
            connection,
            &Target {
                kind: record.kind.clone(),
                uuid: record.uuid.clone(),
            },
            &record.operation_uuid,
            record.purged_at,
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: String,
    pub uuid: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Plan {
    pub operation_uuid: String,
    pub total: i64,
    pub attachments: i64,
    pub bytes: i64,
    pub state: String,
    pub pending: i64,
    pub purged: i64,
    pub skipped: i64,
    pub cleanup_pending: i64,
    pub sync_pending: bool,
    pub remote_pending: i64,
}

fn db_error(_: rusqlite::Error) -> String {
    "PURGE_DATABASE_FAILED".into()
}
fn id(value: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(value).map_err(|_| "PURGE_INVALID_ID")?;
    if parsed.hyphenated().to_string() != value {
        return Err("PURGE_INVALID_ID".into());
    }
    Ok(())
}
fn validate(target: &Target) -> Result<(), String> {
    id(&target.uuid)?;
    if target.kind != "todo" && target.kind != "note" {
        return Err("PURGE_INVALID_KIND".into());
    }
    Ok(())
}

// Both upgraded transports publish terminal evidence before syncing entity bodies.
pub fn require_safe(connection: &Connection) -> Result<(), String> {
    let key: String = connection
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(db_error)?;
    crate::space_activation::admit(connection, &key)?;
    let configured: bool = connection.query_row("SELECT length(trim(endpoint))>0 AND length(trim(bucket))>0 FROM sync_settings WHERE id=1", [], |r| r.get(0)).map_err(db_error)?;
    if configured {
        crate::sync_target::capture(connection)?;
    }
    Ok(())
}

fn epoch(connection: &Connection) -> Result<String, String> {
    Ok(connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key='sync.target.epoch.v1'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?
        .unwrap_or_default())
}

fn rows(
    connection: &Connection,
    sql: &str,
    uuid: &str,
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    let mut statement = connection.prepare(sql).map_err(db_error)?;
    let columns = statement.column_count();
    let result = statement
        .query_map([uuid], |row| {
            (0..columns)
                .map(|i| {
                    Ok(match row.get_ref(i)? {
                        ValueRef::Null => serde_json::Value::Null,
                        ValueRef::Integer(v) => v.into(),
                        ValueRef::Real(_) | ValueRef::Blob(_) => {
                            return Err(rusqlite::Error::InvalidQuery)
                        }
                        ValueRef::Text(v) => serde_json::Value::String(
                            String::from_utf8(v.to_vec())
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        ),
                    })
                })
                .collect::<Result<Vec<_>, rusqlite::Error>>()
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(result)
}

fn fingerprint(connection: &Connection, target: &Target) -> Result<Option<String>, String> {
    let parent = if target.kind == "todo" {
        "SELECT * FROM todos WHERE uuid=?1 AND deleted_at IS NOT NULL"
    } else {
        "SELECT * FROM notes WHERE uuid=?1 AND deleted_at IS NOT NULL"
    };
    let mut snapshot = vec![rows(connection, parent, &target.uuid)?];
    if snapshot[0].is_empty() {
        return Ok(None);
    }
    if target.kind == "todo" {
        snapshot.push(rows(connection, "SELECT uuid,active,record_json FROM task_checklist_items WHERE todo_uuid=?1 ORDER BY uuid", &target.uuid)?);
        snapshot.push(rows(connection, "SELECT uuid,active,record_json FROM recurrence_rules WHERE current_todo_uuid=?1 ORDER BY uuid", &target.uuid)?);
        snapshot.push(rows(
            connection,
            "SELECT uuid,active,record_json FROM task_note_links WHERE todo_uuid=?1 ORDER BY uuid",
            &target.uuid,
        )?);
    } else {
        // Include earlier individually deleted attachments, not just the restore subset.
        snapshot.push(rows(
            connection,
            "SELECT * FROM note_attachments WHERE note_uuid=?1 ORDER BY uuid",
            &target.uuid,
        )?);
        snapshot.push(rows(
            connection,
            "SELECT uuid,active,record_json FROM task_note_links WHERE note_uuid=?1 ORDER BY uuid",
            &target.uuid,
        )?);
        snapshot.push(rows(
            connection,
            "SELECT * FROM note_history WHERE note_uuid=?1 ORDER BY id",
            &target.uuid,
        )?);
    }
    let bytes = serde_json::to_vec(&snapshot).map_err(|_| "PURGE_INVALID_SNAPSHOT")?;
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

pub fn prepare(
    connection: &mut Connection,
    selected: Option<Vec<Target>>,
    now: i64,
) -> Result<Plan, String> {
    if !(0..=MAX_SAFE).contains(&now) {
        return Err("PURGE_INVALID_VERSION".into());
    }
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    require_safe(&tx)?;
    let targets = match selected {
        Some(items) => items,
        None => {
            let mut query = tx.prepare("SELECT 'todo',uuid FROM todos WHERE deleted_at IS NOT NULL UNION ALL SELECT 'note',uuid FROM notes WHERE deleted_at IS NOT NULL ORDER BY 1,2").map_err(db_error)?;
            let result = query
                .query_map([], |r| {
                    Ok(Target {
                        kind: r.get(0)?,
                        uuid: r.get(1)?,
                    })
                })
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            result
        }
    };
    if targets.is_empty() {
        return Err("PURGE_EMPTY".into());
    }
    if targets.len() > 100_000 {
        return Err("PURGE_TOO_MANY".into());
    }
    let mut seen = BTreeSet::new();
    let operation = uuid::Uuid::new_v4().to_string();
    let mut attachments = 0_i64;
    let mut bytes = 0_i64;
    tx.execute("INSERT INTO purge_plans(operation_uuid,target_epoch,created_at,total,attachments,bytes,state) VALUES(?1,?2,?3,?4,0,0,'prepared')",
        params![operation,epoch(&tx)?,now,targets.len() as i64]).map_err(db_error)?;
    for target in targets {
        validate(&target)?;
        if !seen.insert((target.kind.clone(), target.uuid.clone())) {
            return Err("PURGE_DUPLICATE_TARGET".into());
        }
        let digest = fingerprint(&tx, &target)?.ok_or("PURGE_CONFLICT")?;
        if target.kind == "note" {
            let (count, size): (i64, i64) = tx.query_row("SELECT COUNT(*),COALESCE(SUM(byte_size+COALESCE(preview_byte_size,0)),0) FROM note_attachments WHERE note_uuid=?1", [&target.uuid],
                |r| Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
            attachments = attachments
                .checked_add(count)
                .ok_or("PURGE_INVALID_VERSION")?;
            bytes = bytes
                .checked_add(size)
                .filter(|n| *n <= MAX_SAFE)
                .ok_or("PURGE_INVALID_VERSION")?;
        }
        tx.execute(
            "INSERT INTO purge_targets(operation_uuid,kind,uuid,fingerprint) VALUES(?1,?2,?3,?4)",
            params![operation, target.kind, target.uuid, digest],
        )
        .map_err(db_error)?;
    }
    tx.execute(
        "UPDATE purge_plans SET attachments=?1,bytes=?2 WHERE operation_uuid=?3",
        params![attachments, bytes, operation],
    )
    .map_err(db_error)?;
    let result = status(&tx, &operation)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}

pub fn status(connection: &Connection, operation: &str) -> Result<Plan, String> {
    id(operation)?;
    let mut result = connection
        .query_row(
            "SELECT total,attachments,bytes,state FROM purge_plans WHERE operation_uuid=?1",
            [operation],
            |r| {
                Ok(Plan {
                    operation_uuid: operation.into(),
                    total: r.get(0)?,
                    attachments: r.get(1)?,
                    bytes: r.get(2)?,
                    state: r.get(3)?,
                    pending: 0,
                    purged: 0,
                    skipped: 0,
                    cleanup_pending: 0,
                    sync_pending: false,
                    remote_pending: 0,
                })
            },
        )
        .optional()
        .map_err(db_error)?
        .ok_or("PURGE_PLAN_MISSING")?;
    let mut query = connection
        .prepare(
            "SELECT outcome,COUNT(*) FROM purge_targets WHERE operation_uuid=?1 GROUP BY outcome",
        )
        .map_err(db_error)?;
    let groups = query
        .query_map([operation], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(db_error)?;
    for group in groups {
        let (outcome, count) = group.map_err(db_error)?;
        match outcome.as_str() {
            "pending" => result.pending += count,
            "purged" | "already_purged" => result.purged += count,
            _ => result.skipped += count,
        }
    }
    result.cleanup_pending = connection.query_row("SELECT COUNT(*) FROM purge_cleanup c JOIN purge_targets t ON t.uuid=c.note_uuid AND t.kind='note' WHERE t.operation_uuid=?1 AND c.local_done=0", [operation], |r| r.get(0)).map_err(db_error)?;
    result.remote_pending = connection.query_row("SELECT COUNT(*) FROM purge_cleanup c JOIN purge_targets t ON t.uuid=c.note_uuid AND t.kind='note' WHERE t.operation_uuid=?1 AND c.remote_done=0", [operation], |r| r.get(0)).map_err(db_error)?;
    let active = crate::space_activation::is_active(connection)?;
    let was_synced: bool = connection
        .query_row(
            "SELECT length(target_epoch)>0 FROM purge_plans WHERE operation_uuid=?1",
            [operation],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if result.purged > 0 && (active || was_synced) {
        result.sync_pending = connection.query_row("SELECT revision<>synced_revision OR etag IS NULL OR EXISTS(SELECT 1 FROM sync_runtime_state WHERE last_result<>'success' OR dirty_domains<>'[]') FROM lifecycle_sync_state WHERE id=1", [], |r| r.get::<_, bool>(0)).map_err(db_error)?;
    }
    Ok(result)
}

pub fn unfinished(connection: &Connection) -> Result<Option<Plan>, String> {
    let operation: Option<String> = connection.query_row("SELECT p.operation_uuid FROM purge_plans p WHERE state='running' OR EXISTS(SELECT 1 FROM purge_targets t JOIN purge_cleanup c ON t.kind='note' AND t.uuid=c.note_uuid WHERE t.operation_uuid=p.operation_uuid AND (c.local_done=0 OR c.remote_done=0)) OR (state='complete' AND length(target_epoch)>0 AND EXISTS(SELECT 1 FROM lifecycle_sync_state WHERE revision<>synced_revision OR etag IS NULL OR EXISTS(SELECT 1 FROM sync_runtime_state WHERE last_result<>'success' OR dirty_domains<>'[]'))) ORDER BY created_at DESC,p.operation_uuid LIMIT 1", [], |r| r.get(0)).optional().map_err(db_error)?;
    operation.map(|id| status(connection, &id)).transpose()
}

pub fn cleanup(
    connection: &Connection,
    assets: &crate::note_asset_store::NoteAssetStore,
    operation: &str,
) -> Result<(), String> {
    id(operation)?;
    let mut query = connection.prepare("SELECT c.attachment_uuid FROM purge_cleanup c WHERE c.local_done=0 AND EXISTS(SELECT 1 FROM purge_targets t WHERE t.operation_uuid=?1 AND t.kind='note' AND t.uuid=c.note_uuid) ORDER BY c.local_attempts,c.attachment_uuid LIMIT 100").map_err(db_error)?;
    let targets = query
        .query_map([operation], |r| r.get::<_, String>(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    for uuid in targets {
        // Never trust persisted paths for deletion; the asset store derives a UUID-scoped path.
        if assets.delete_asset(&uuid).is_ok() {
            connection.execute("UPDATE purge_cleanup SET local_done=1,original_path=NULL,preview_path=NULL WHERE attachment_uuid=?1", [uuid]).map_err(db_error)?;
        } else {
            connection.execute("UPDATE purge_cleanup SET local_attempts=local_attempts+1 WHERE attachment_uuid=?1", [uuid]).map_err(db_error)?;
        }
    }
    Ok(())
}

fn contains_identity(value: &serde_json::Value, uuid: &str, depth: usize) -> Result<bool, String> {
    if depth > 16 {
        return Err("PURGE_RECEIPT_INVALID".into());
    }
    Ok(match value {
        serde_json::Value::String(v) => {
            if v == uuid {
                true
            } else if v.starts_with('{') || v.starts_with('[') {
                match serde_json::from_str::<serde_json::Value>(v) {
                    Ok(nested) => contains_identity(&nested, uuid, depth + 1)?,
                    Err(_) => false,
                }
            } else {
                false
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                if contains_identity(value, uuid, depth)? {
                    return Ok(true);
                }
            }
            false
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                if key == uuid || contains_identity(value, uuid, depth)? {
                    return Ok(true);
                }
            }
            false
        }
        _ => false,
    })
}

fn scrub_receipts(connection: &Connection, uuid: &str) -> Result<(), String> {
    let pending: Option<String> = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key='task.batch.pending.v1'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if let Some(payload) = pending {
        let parsed = serde_json::from_str(&payload).map_err(|_| "PURGE_RECEIPT_INVALID")?;
        if contains_identity(&parsed, uuid, 0)? {
            return Err("PURGE_PENDING_CREATION".into());
        }
    }
    for table in ["task_checklist_operations", "task_template_operations"] {
        let mut query = connection
            .prepare(&format!("SELECT operation_uuid,payload FROM {table}"))
            .map_err(db_error)?;
        let receipts = query
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        for (operation, payload) in receipts {
            let parsed: serde_json::Value =
                serde_json::from_str(&payload).map_err(|_| "PURGE_RECEIPT_INVALID")?;
            if contains_identity(&parsed, uuid, 0)? {
                // Keeping a scrubbed receipt prevents an old operation from being recreated.
                connection.execute(&format!("UPDATE {table} SET payload='{{\"purged\":true}}' WHERE operation_uuid=?1"), [operation]).map_err(db_error)?;
            }
        }
    }
    let mut query = connection.prepare("SELECT key,value FROM app_metadata WHERE key LIKE 'checklist.%' OR key LIKE 'recurrence.edit.v1:%' OR key LIKE 'task.batch.create.v1:%'").map_err(db_error)?;
    let receipts = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    for (key, payload) in receipts {
        let parsed: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| "PURGE_RECEIPT_INVALID")?;
        if contains_identity(&parsed, uuid, 0)? {
            connection
                .execute(
                    "UPDATE app_metadata SET value='{\"purged\":true}' WHERE key=?1",
                    [key],
                )
                .map_err(db_error)?;
        }
    }
    Ok(())
}

pub(crate) fn erase(
    connection: &Connection,
    target: &Target,
    operation: &str,
    now: i64,
) -> Result<(), String> {
    validate(target)?;
    id(operation)?;
    if !(0..=MAX_SAFE).contains(&now) {
        return Err("PURGE_INVALID_VERSION".into());
    }
    if target.kind == "todo" {
        let running: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_rules WHERE current_todo_uuid=?1 AND active=1)", [&target.uuid], |r| r.get(0)).map_err(db_error)?;
        if running {
            return Err("PURGE_RULE_ACTIVE".into());
        }
        scrub_receipts(connection, &target.uuid)?;
        connection
            .execute(
                "DELETE FROM task_checklist_items WHERE todo_uuid=?1",
                [&target.uuid],
            )
            .map_err(db_error)?;
        connection
            .execute(
                "DELETE FROM task_note_links WHERE todo_uuid=?1",
                [&target.uuid],
            )
            .map_err(db_error)?;
        connection
            .execute("DELETE FROM todos WHERE uuid=?1", [&target.uuid])
            .map_err(db_error)?;
    } else {
        crate::purge_remote::capture_local(connection, &target.uuid)?;
        connection.execute("INSERT OR IGNORE INTO purge_cleanup(attachment_uuid,note_uuid,original_path,preview_path,remote_done) SELECT uuid,note_uuid,local_original_path,local_preview_path,CASE WHEN remote_uploaded=0 THEN 1 ELSE 0 END FROM note_attachments WHERE note_uuid=?1", [&target.uuid]).map_err(db_error)?;
        connection
            .execute(
                "DELETE FROM note_attachments WHERE note_uuid=?1",
                [&target.uuid],
            )
            .map_err(db_error)?;
        connection
            .execute(
                "DELETE FROM task_note_links WHERE note_uuid=?1",
                [&target.uuid],
            )
            .map_err(db_error)?;
        connection
            .execute(
                "DELETE FROM note_history WHERE note_uuid=?1",
                [&target.uuid],
            )
            .map_err(db_error)?;
        connection
            .execute("DELETE FROM notes WHERE uuid=?1", [&target.uuid])
            .map_err(db_error)?;
    }
    connection.execute("INSERT OR IGNORE INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES(?1,?2,?3,?4)", params![target.kind,target.uuid,operation,now]).map_err(db_error)?;
    Ok(())
}

pub fn execute_batch(
    connection: &mut Connection,
    operation: &str,
    now: i64,
) -> Result<Plan, String> {
    id(operation)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)?;
    require_safe(&tx)?;
    let source: String = tx
        .query_row(
            "SELECT target_epoch FROM purge_plans WHERE operation_uuid=?1",
            [operation],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?
        .ok_or("PURGE_PLAN_MISSING")?;
    if source != epoch(&tx)? {
        return Err("PURGE_TARGET_CHANGED".into());
    }
    let targets = {
        let mut query = tx.prepare("SELECT kind,uuid,fingerprint FROM purge_targets WHERE operation_uuid=?1 AND outcome='pending' ORDER BY kind,uuid LIMIT 50").map_err(db_error)?;
        let result = query
            .query_map([operation], |r| {
                Ok((
                    Target {
                        kind: r.get(0)?,
                        uuid: r.get(1)?,
                    },
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        result
    };
    for (target, expected) in targets {
        let terminal: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind=?1 AND uuid=?2)",
                params![target.kind, target.uuid],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let current = fingerprint(&tx, &target)?;
        let outcome = if terminal {
            "already_purged"
        } else if current.is_none() {
            "skipped_missing"
        } else if current.as_deref() != Some(expected.as_str()) {
            "skipped_conflict"
        } else {
            match erase(&tx, &target, operation, now) {
                Ok(()) => "purged",
                Err(code) if code == "PURGE_RULE_ACTIVE" => "skipped_rule",
                Err(code) => return Err(code),
            }
        };
        tx.execute(
            "UPDATE purge_targets SET outcome=?1 WHERE operation_uuid=?2 AND kind=?3 AND uuid=?4",
            params![outcome, operation, target.kind, target.uuid],
        )
        .map_err(db_error)?;
    }
    tx.execute("UPDATE purge_plans SET state=CASE WHEN EXISTS(SELECT 1 FROM purge_targets WHERE operation_uuid=?1 AND outcome='pending') THEN 'running' ELSE 'complete' END WHERE operation_uuid=?1", [operation]).map_err(db_error)?;
    let result = status(&tx, operation)?;
    tx.commit().map_err(db_error)?;
    Ok(result)
}
