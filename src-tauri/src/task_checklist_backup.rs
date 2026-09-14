//! Portable checklist documents, never local sync or editor receipts.
use crate::{task_checklist_protocol as protocol, task_checklist_store as store};
use rusqlite::Transaction;
use serde::Deserialize;

pub(crate) fn deserialize_items<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<protocol::ItemsDocument>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    protocol::parse_items(&raw.to_string())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn deserialize_definitions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<protocol::DefinitionsDocument>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    protocol::parse_definitions(&raw.to_string())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn restore(
    tx: &Transaction<'_>,
    items: Option<&protocol::ItemsDocument>,
    definitions: Option<&protocol::DefinitionsDocument>,
) -> Result<(), String> {
    match (items, definitions) {
        (None, None) => Ok(()),
        (Some(items), Some(definitions)) => {
            // Protocol merge protects local tombstones, including against future-clock active copies.
            store::merge_in_transaction(tx, items, definitions)?;
            tx.execute("DELETE FROM task_checklist_operations", [])
                .map_err(|e| format!("CHECKLIST_DATABASE: {e}"))?;
            tx.execute("UPDATE task_checklist_sync_state SET revision=revision+1,synced_revision=0,etag=NULL,generation=generation+1", [])
                .map_err(|e| format!("CHECKLIST_DATABASE: {e}"))?;
            store::read_in_transaction(tx)?;
            Ok(())
        }
        _ => Err("INVALID_CHECKLIST_BACKUP_VERSION".into()),
    }
}
