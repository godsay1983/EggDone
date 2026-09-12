//! Portable links only. Import is not an explicit relink command or a sync receipt.
use crate::{task_note_link_protocol as protocol, task_note_link_store as store};
use rusqlite::Transaction;
use serde::Deserialize;

pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<protocol::LinkDocument>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    protocol::parse_document(&raw.to_string())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn restore(
    db: &Transaction<'_>,
    incoming: Option<&protocol::LinkDocument>,
    now: i64,
) -> Result<(), String> {
    if let Some(incoming) = incoming {
        protocol::encode_document(incoming)?;
        let local = store::snapshot(db)?.document;
        let tombstones: std::collections::HashSet<&str> = local
            .links
            .iter()
            .filter(|link| link.deleted_at.is_some())
            .map(|link| link.uuid.as_str())
            .collect();
        let filtered = protocol::LinkDocument {
            format_version: 1,
            links: incoming
                .links
                .iter()
                .filter(|link| {
                    link.deleted_at.is_some() || !tombstones.contains(link.uuid.as_str())
                })
                .cloned()
                .collect(),
        };
        store::merge_in_transaction(db, &filtered)?;
    }
    // Legacy imports may contain entity deletions even though they carry no link metadata.
    crate::task_note_link_sync::reconcile_in_transaction(db, now)?;
    if incoming.is_some() {
        db.execute("UPDATE task_note_link_sync_state SET revision=revision+1,synced_revision=0,etag=NULL WHERE id=1", [])
            .map_err(|e| format!("TASK_NOTE_LINK_DATABASE: {e}"))?;
    }
    Ok(())
}
