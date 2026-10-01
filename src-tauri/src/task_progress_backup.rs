use crate::{task_progress_protocol as protocol, task_progress_store as store};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<protocol::Document>, D::Error> {
    let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
    protocol::parse(raw.get())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn validate(version: u32, document: Option<&protocol::Document>) -> Result<(), String> {
    match (version, document) {
        (1..=8, None) => Ok(()),
        (9, Some(document)) => protocol::validate(document),
        _ => Err("PROGRESS_INVALID".into()),
    }
}

pub(crate) fn restore(
    connection: &Connection,
    document: Option<&protocol::Document>,
) -> Result<(), String> {
    if let Some(document) = document {
        store::merge_in_transaction(connection, document)?;
    }
    // Even an old backup invalidates in-flight uploads and local UI retry receipts.
    // ETag/ACK are not portable; retaining our own target ETag preserves missing-object protection.
    connection
        .execute(
            "UPDATE task_progress_sync_state SET generation=generation+1 WHERE id=1",
            [],
        )
        .map_err(store::error)?;
    connection
        .execute("DELETE FROM task_progress_operations", [])
        .map_err(store::error)?;
    connection
        .execute("DELETE FROM task_progress_notices", [])
        .map_err(store::error)?;
    store::read_in_transaction(connection)?;
    Ok(())
}

#[derive(Default)]
pub(crate) struct Changes {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

pub(crate) fn changes(
    local: &protocol::Document,
    incoming: Option<&protocol::Document>,
    purged: &HashSet<String>,
) -> Result<Changes, String> {
    let Some(incoming) = incoming else {
        return Ok(Changes::default());
    };
    let merged = protocol::merge(
        &protocol::filter_purged(local, purged)?,
        &protocol::filter_purged(incoming, purged)?,
    )?;
    let before: HashMap<_, _> = local.entries.iter().map(|e| (&e.uuid, e)).collect();
    let after: HashMap<_, _> = merged.entries.iter().map(|e| (&e.uuid, e)).collect();
    let mut result = Changes::default();
    let incoming_ids: HashSet<_> = incoming.entries.iter().map(|e| &e.uuid).collect();
    for entry in &incoming.entries {
        let previous = before.get(&entry.uuid);
        match after.get(&entry.uuid) {
            None if previous.is_some() => result.deleted += 1,
            None => result.unchanged += 1,
            Some(current) if previous == Some(current) => result.unchanged += 1,
            Some(current) if current.deleted_at.is_some() => result.deleted += 1,
            Some(_) if previous.is_none() => result.added += 1,
            Some(_) => result.updated += 1,
        }
    }
    // A terminal may remove a local record not present in the incoming domain.
    for entry in &local.entries {
        if purged.contains(&entry.task_uuid) && !incoming_ids.contains(&entry.uuid) {
            result.deleted += 1;
        }
    }
    Ok(result)
}
