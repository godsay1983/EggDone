//! Two independent wire domains, one atomic local merge and dependency snapshot.
use crate::{
    recurrence_protocol, recurrence_store, sync, sync_target, task_checklist_inheritance,
    task_checklist_protocol as p, task_checklist_store as store,
};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Items,
    Definitions,
}
impl Domain {
    pub fn name(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::Definitions => "definitions",
        }
    }
    pub fn canonical(self, raw: &str) -> Result<String, String> {
        match self {
            Self::Items => p::encode_items(&p::parse_items(raw)?),
            Self::Definitions => p::encode_definitions(&p::parse_definitions(raw)?),
        }
    }
    pub fn empty(self) -> String {
        format!("{{\"format_version\":1,\"{}\":[]}}", self.name())
    }
    pub fn is_empty(self, raw: &str) -> Result<bool, String> {
        Ok(match self {
            Self::Items => p::parse_items(raw)?.items.is_empty(),
            Self::Definitions => p::parse_definitions(raw)?.definitions.is_empty(),
        })
    }
    pub fn object_key(self, todo: &str, occupied: &[String]) -> Result<String, String> {
        // Reuse the existing exact path validation; the reserved link name is not the checklist key.
        crate::task_note_link_protocol::object_key(todo, &[])?;
        let directory = todo.rfind('/').map_or("", |i| &todo[..=i]);
        let key = format!("{directory}task-checklist-{}.json", self.name());
        if key == todo || key.len() > 1024 || occupied.contains(&key) {
            return Err("CHECKLIST_KEY_COLLISION".into());
        }
        Ok(key)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub items: String,
    pub definitions: String,
    pub todo_json: String,
    pub rules_json: String,
    pub item_revision: i64,
    pub definition_revision: i64,
    pub item_generation: i64,
    pub definition_generation: i64,
    pub todo_revision: i64,
    pub rule_revision: i64,
    pub rules_ready: bool,
    epoch: String,
    now: i64,
}
fn error(e: rusqlite::Error) -> String {
    format!("CHECKLIST_SYNC_DATABASE: {e}")
}
fn current(tx: &Transaction<'_>, epoch: &str) -> Result<bool, String> {
    Ok(!epoch.is_empty() && !epoch.starts_with("pending:") && sync_target::is_current(tx, epoch)?)
}
fn capture(tx: &Transaction<'_>, epoch: &str, now: i64) -> Result<Snapshot, String> {
    let s = store::read_in_transaction(tx)?;
    let rules = recurrence_store::snapshot(tx)?;
    let todo = sync::build_document(tx, now)?;
    sync::validate_document(&todo)?;
    let todo_revision = tx
        .query_row(
            "SELECT todos_dirty_version FROM sync_runtime_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(error)?;
    Ok(Snapshot {
        items: p::encode_items(&s.items)?,
        definitions: p::encode_definitions(&s.definitions)?,
        todo_json: serde_json::to_string(&todo).map_err(|_| "CHECKLIST_SNAPSHOT_INVALID")?,
        rules_json: recurrence_protocol::encode_document(&rules.document)?,
        item_revision: s.item_state.revision,
        definition_revision: s.definition_state.revision,
        item_generation: s.item_state.generation,
        definition_generation: s.definition_state.generation,
        todo_revision,
        rule_revision: rules.revision,
        // A legacy target may have no rule object. Empty local rules carry no dependency to upload.
        rules_ready: rules.document.rules.is_empty() || rules.revision == rules.synced_revision,
        epoch: epoch.into(),
        now,
    })
}
pub fn prepare(
    db: &mut Connection,
    epoch: &str,
    items: &str,
    definitions: &str,
    now: i64,
) -> Result<Snapshot, String> {
    if !(0..=p::MAX_CLOCK).contains(&now) {
        return Err("CHECKLIST_SNAPSHOT_INVALID".into());
    }
    let items = p::parse_items(items)?;
    let definitions = p::parse_definitions(definitions)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, epoch)? {
        return Err("CHECKLIST_CONFIG_CHANGED".into());
    }
    store::merge_in_transaction(&tx, &items, &definitions)?;
    task_checklist_inheritance::repair_in_transaction(&tx, None)?;
    let s = capture(&tx, epoch, now)?;
    tx.commit().map_err(error)?;
    Ok(s)
}
pub fn is_current(db: &mut Connection, s: &Snapshot) -> Result<bool, String> {
    check(db, s, None)
}
pub fn acknowledge(
    db: &mut Connection,
    s: &Snapshot,
    domain: Domain,
    etag: Option<&str>,
) -> Result<bool, String> {
    if let Some(tag) = etag {
        if !crate::task_checklist_transport::valid_etag(tag) {
            return Err("CHECKLIST_ETAG_REQUIRED".into());
        }
    } else if !domain.is_empty(match domain {
        Domain::Items => &s.items,
        Domain::Definitions => &s.definitions,
    })? {
        return Err("CHECKLIST_NOT_EMPTY".into());
    }
    check(db, s, Some((domain, etag)))
}
fn check(
    db: &mut Connection,
    s: &Snapshot,
    ack: Option<(Domain, Option<&str>)>,
) -> Result<bool, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    if !current(&tx, &s.epoch)? || capture(&tx, &s.epoch, s.now)? != *s {
        return Ok(false);
    }
    if let Some((domain, etag)) = ack {
        let revision = match domain {
            Domain::Items => s.item_revision,
            Domain::Definitions => s.definition_revision,
        };
        tx.execute(
            "UPDATE task_checklist_sync_state SET synced_revision=?1,etag=?2 WHERE domain=?3",
            params![revision, etag, domain.name()],
        )
        .map_err(error)?;
    }
    tx.commit().map_err(error)?;
    Ok(true)
}
