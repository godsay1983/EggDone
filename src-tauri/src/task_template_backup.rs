use crate::{task_template_protocol as protocol, task_template_store as store};
use rusqlite::Transaction;
use serde::Deserialize;

pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<protocol::TemplatesDocument>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    protocol::parse(&raw.to_string())
        .map(Some)
        .map_err(serde::de::Error::custom)
}
pub(crate) fn validate(
    version: u32,
    document: Option<&protocol::TemplatesDocument>,
) -> Result<(), String> {
    match (version, document) {
        (1..=4, None) => Ok(()),
        (5, Some(doc)) => protocol::encode(doc).map(|_| ()),
        _ => Err("INVALID_TEMPLATE_BACKUP_VERSION".into()),
    }
}
pub(crate) fn restore(
    tx: &Transaction<'_>,
    document: Option<&protocol::TemplatesDocument>,
) -> Result<(), String> {
    let Some(document) = document else {
        return Ok(());
    };
    store::merge_in_transaction(tx, document)?;
    tx.execute("DELETE FROM task_template_operations", [])
        .map_err(|_| "TEMPLATE_BACKUP_DATABASE")?;
    tx.execute("UPDATE task_template_sync_state SET revision=revision+1,synced_revision=0,etag=NULL,generation=generation+1", [])
        .map_err(|_| "TEMPLATE_BACKUP_DATABASE")?;
    store::read_in_transaction(tx)?;
    Ok(())
}
