//! Portable rule metadata. Restore never advances a rule or replays an editor command.
use crate::{
    recurrence::{recurrence_occurrence_key, recurrence_todo_uuid, RecurrenceOccurrence},
    recurrence_links::{inspect_links, LinkTodo},
    recurrence_protocol::{encode_document, RecurrenceDocument},
    recurrence_store,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleBackup {
    pub document: RecurrenceDocument,
    pub instances: Vec<BackupInstance>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupInstance {
    pub todo_uuid: String,
    pub rule_uuid: String,
    pub occurrence_key: String,
    pub planned_date: String,
    pub planned_due_at: Option<i64>,
    pub purged: bool,
}

#[derive(Deserialize, Serialize)]
struct LocalReceipt {
    rule_uuid: String,
    occurrence_key: String,
    planned_date: String,
    planned_due_at: Option<i64>,
    projected_due_at: Option<i64>,
}

fn db(e: rusqlite::Error) -> String {
    format!("RECURRENCE_DATABASE: {e}")
}
const INVALID: &str = "INVALID_RECURRENCE_BACKUP";

pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<RuleBackup>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    let mut backup: RuleBackup =
        serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
    backup.document = crate::recurrence_protocol::parse_document(&raw["document"].to_string())
        .map_err(serde::de::Error::custom)?;
    if raw["instances"].as_array().is_none_or(|items| {
        items
            .iter()
            .any(|item| item.get("planned_due_at").is_none())
    }) {
        return Err(serde::de::Error::custom(INVALID));
    }
    validate(&backup).map_err(serde::de::Error::custom)?;
    Ok(Some(backup))
}

pub(crate) fn validate(backup: &RuleBackup) -> Result<(), String> {
    encode_document(&backup.document)?;
    if backup.instances.len() > 100_000
        || serde_json::to_string(backup)
            .map_err(|_| INVALID)?
            .encode_utf16()
            .count()
            > 16 * 1024 * 1024
    {
        return Err(INVALID.into());
    }
    let mut ids = HashSet::new();
    for item in &backup.instances {
        let rule = backup
            .document
            .rules
            .iter()
            .find(|r| r.uuid == item.rule_uuid)
            .ok_or(INVALID)?;
        let index = item
            .occurrence_key
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or(INVALID)?;
        let occurrence = RecurrenceOccurrence {
            date: item.planned_date.clone(),
            index,
        };
        if index < 2
            || !ids.insert(&item.todo_uuid)
            || item.occurrence_key
                != recurrence_occurrence_key(&rule.uuid, &rule.schedule, &occurrence)?
            || item.todo_uuid != recurrence_todo_uuid(&rule.uuid, &rule.schedule, &occurrence)?
            || (rule.schedule.local_time_minutes.is_none() != item.planned_due_at.is_none())
            || item
                .planned_due_at
                .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
        {
            return Err(INVALID.into());
        }
    }
    Ok(())
}

pub(crate) fn export(connection: &Connection) -> Result<RuleBackup, String> {
    let document = recurrence_store::snapshot(connection)?.document;
    let mut statement = connection.prepare("SELECT key,value,EXISTS(SELECT 1 FROM todos WHERE uuid=substr(key,24)) FROM app_metadata WHERE key LIKE 'recurrence.instance.v1:%' ORDER BY key").map_err(db)?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })
        .map_err(db)?;
    let mut instances = Vec::new();
    for row in rows {
        let (key, raw, exists) = row.map_err(db)?;
        let receipt: LocalReceipt = serde_json::from_str(&raw).map_err(|_| INVALID)?;
        instances.push(BackupInstance {
            todo_uuid: key.trim_start_matches("recurrence.instance.v1:").into(),
            rule_uuid: receipt.rule_uuid,
            occurrence_key: receipt.occurrence_key,
            planned_date: receipt.planned_date,
            planned_due_at: receipt.planned_due_at,
            purged: !exists,
        });
    }
    let backup = RuleBackup {
        document,
        instances,
    };
    validate(&backup)?;
    Ok(backup)
}

// Reject incompatible ownership before committing any domain; missing/archived sources stay dormant.
pub(crate) fn check_links(connection: &Connection) -> Result<(), String> {
    let document = recurrence_store::snapshot(connection)?.document;
    let mut statement = connection.prepare("SELECT uuid,repeat_rule,repeat_series_uuid,completed,deleted_at,archived_at,updated_at FROM todos WHERE uuid IN (SELECT current_todo_uuid FROM recurrence_rules WHERE active=1)").map_err(db)?;
    let todos = statement
        .query_map([], |r| {
            Ok(LinkTodo {
                uuid: r.get(0)?,
                repeat_rule: r.get(1)?,
                repeat_series_uuid: r.get(2)?,
                completed: r.get(3)?,
                deleted_at: r.get(4)?,
                archived_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?;
    if inspect_links(&document, &todos)?
        .links
        .iter()
        .any(|l| l.state == "conflict")
    {
        return Err("RECURRENCE_LINK_CONFLICT".into());
    }
    Ok(())
}

pub(crate) fn restore(connection: &Connection, backup: &RuleBackup) -> Result<(), String> {
    validate(backup)?;
    recurrence_store::merge_in_transaction(connection, &backup.document)?;
    for item in &backup.instances {
        let key = format!("recurrence.instance.v1:{}", item.todo_uuid);
        let old: Option<String> = connection
            .query_row("SELECT value FROM app_metadata WHERE key=?1", [&key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db)?;
        if let Some(raw) = old {
            let receipt: LocalReceipt = serde_json::from_str(&raw).map_err(|_| INVALID)?;
            if receipt.rule_uuid != item.rule_uuid || receipt.occurrence_key != item.occurrence_key
            {
                return Err("RECURRENCE_LINK_CONFLICT".into());
            }
        } else {
            let receipt = LocalReceipt {
                rule_uuid: item.rule_uuid.clone(),
                occurrence_key: item.occurrence_key.clone(),
                planned_date: item.planned_date.clone(),
                planned_due_at: item.planned_due_at,
                projected_due_at: item.planned_due_at,
            };
            connection
                .execute(
                    "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                    params![key, serde_json::to_string(&receipt).map_err(|_| INVALID)?],
                )
                .map_err(db)?;
        }
    }
    check_links(connection)?;
    connection.execute("UPDATE recurrence_sync_state SET revision=revision+1,synced_revision=0,etag=NULL WHERE id=1",[]).map_err(db)?;
    connection
        .execute(
            "UPDATE sync_runtime_state SET todos_dirty_version=todos_dirty_version+1 WHERE id=1",
            [],
        )
        .map_err(db)?;
    crate::sync_runtime_state::retain_todos_pending_in_transaction(connection)?;
    // A restored snapshot must not satisfy an old local editor retry fingerprint.
    connection
        .execute(
            "DELETE FROM app_metadata WHERE key LIKE 'recurrence.edit.v1:%'",
            [],
        )
        .map_err(db)?;
    Ok(())
}

pub(crate) fn reject_purged_restore(
    connection: &Connection,
    todo_uuid: &str,
) -> Result<(), String> {
    let key = format!("recurrence.instance.v1:{todo_uuid}");
    let purged: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1) AND NOT EXISTS(SELECT 1 FROM todos WHERE uuid=?2)",params![key,todo_uuid],|r|r.get(0)).map_err(db)?;
    if purged {
        return Err("RECURRENCE_OCCURRENCE_MISSING".into());
    }
    Ok(())
}
