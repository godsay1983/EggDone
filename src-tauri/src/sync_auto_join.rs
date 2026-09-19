//! Follow an existing immutable association without publishing seeds or migrating a space.
use crate::{
    commands::lock_database,
    db::{now_millis, Database},
    lifecycle_sync,
    note_asset_store::NoteAssetStore,
    note_attachment_sync::{self, SyncNoteAttachment},
    s3_sync::{self, PreparedManualSync, RemoteAssetState, SyncRuntime},
    space_activation::{self as space, Claim},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::future::Future;

const LIMIT: usize = 16 * 1024 * 1024;

fn fail(_: impl std::fmt::Display) -> String {
    "SYNC_AUTO_JOIN_INVALID".into()
}
fn db_error(_: rusqlite::Error) -> String {
    "SYNC_AUTO_JOIN_DATABASE".into()
}
fn guard(db: &Database, source: &PreparedManualSync) -> Result<(), String> {
    let c = lock_database(db)?;
    source
        .require_current(&c)
        .map_err(|_| "SYNC_AUTO_JOIN_TARGET_CHANGED".into())
}

// A transport boundary keeps regressions fully in-memory; production uses the normal S3 services.
trait Io: Sync {
    fn read(
        &self,
        p: &PreparedManualSync,
        key: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, String>> + Send;
    fn head(
        &self,
        p: &PreparedManualSync,
        a: &Asset,
    ) -> impl Future<Output = Result<RemoteAssetState, String>> + Send;
    fn download(
        &self,
        p: &PreparedManualSync,
        a: &Asset,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + Send;
    fn upload(
        &self,
        p: &PreparedManualSync,
        a: &Asset,
        bytes: &[u8],
    ) -> impl Future<Output = Result<(), String>> + Send;
    fn local(&self, a: &Asset) -> Result<Vec<u8>, String>;
}
struct Live<'a> {
    runtime: &'a SyncRuntime,
    assets: Option<&'a NoteAssetStore>,
}
impl Io for Live<'_> {
    async fn read(
        &self,
        p: &PreparedManualSync,
        key: &str,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        p.read_join_object(key, limit).await
    }
    async fn head(&self, p: &PreparedManualSync, a: &Asset) -> Result<RemoteAssetState, String> {
        s3_sync::head_asset_object(p, &a.uuid, &a.name)
            .await
            .map_err(asset_error)
    }
    async fn download(&self, p: &PreparedManualSync, a: &Asset) -> Result<Vec<u8>, String> {
        s3_sync::download_asset_bytes(self.runtime, p, &a.uuid, &a.name, a.size, &a.hash)
            .await
            .map_err(asset_error)
    }
    async fn upload(&self, p: &PreparedManualSync, a: &Asset, bytes: &[u8]) -> Result<(), String> {
        s3_sync::upload_immutable_asset(self.runtime, p, &a.uuid, &a.name, bytes, &a.mime, &a.hash)
            .await
            .map(|_| ())
            .map_err(asset_error)
    }
    fn local(&self, a: &Asset) -> Result<Vec<u8>, String> {
        self.assets
            .ok_or("SYNC_AUTO_JOIN_ASSET_MISSING")?
            .read_asset_file(&a.uuid, &a.name, a.size, &a.hash)
            .map_err(asset_error)
    }
}
fn asset_error(error: String) -> String {
    if error.contains("凭据") || error.contains("权限") {
        "SYNC_AUTO_JOIN_DENIED".into()
    } else {
        "SYNC_AUTO_JOIN_ASSET_FAILED".into()
    }
}

async fn read(
    io: &impl Io,
    db: &Database,
    source: &PreparedManualSync,
    key: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, String> {
    guard(db, source)?;
    let result = io.read(source, key, limit).await;
    guard(db, source)?;
    result
}

async fn association(
    io: &impl Io,
    db: &Database,
    p: &PreparedManualSync,
    pin: bool,
) -> Result<Option<Claim>, String> {
    if p.is_versioned_space().map_err(fail)? {
        return Ok(None);
    }
    let pin_key = format!("sync.auto-join.claim.v1:{}", p.epoch());
    let previous: Option<String> = lock_database(db)?
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [&pin_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(bytes) = read(io, db, p, &space::claim_key(p.main_key()), LIMIT).await? else {
        if previous.is_some() {
            return Err("SYNC_AUTO_JOIN_ASSOCIATION_CHANGED".into());
        }
        return Ok(None);
    };
    let claim = Claim::parse(&bytes).map_err(fail)?;
    {
        let c = lock_database(db)?;
        validate_binding(&c, p, &claim)?;
        if let Some(previous) = previous {
            if Claim::parse(previous.as_bytes()).map_err(fail)? != claim {
                return Err("SYNC_AUTO_JOIN_ASSOCIATION_CHANGED".into());
            }
        }
        if pin {
            c.execute(
                "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,?2)",
                params![
                    pin_key,
                    String::from_utf8(claim.raw().map_err(fail)?).map_err(fail)?
                ],
            )
            .map_err(db_error)?;
        }
    }
    ready(io, db, p, &claim).await?;
    Ok(Some(claim))
}
fn validate_binding(c: &Connection, p: &PreparedManualSync, claim: &Claim) -> Result<(), String> {
    p.require_current(c)
        .map_err(|_| "SYNC_AUTO_JOIN_TARGET_CHANGED")?;
    let configured: String = c
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(db_error)?;
    if configured != p.main_key()
        || claim.1 != p.main_key()
        || !s3_sync::matches_auto_join_source_binding(c, &claim.plan().map_err(fail)?.source)
            .map_err(fail)?
    {
        return Err("SYNC_AUTO_JOIN_BINDING".into());
    }
    let main = claim.main().map_err(fail)?;
    if !crate::sync_space::scope(&main)
        .map_err(fail)?
        .is_some_and(|(_, d)| d == "todos")
    {
        return Err("SYNC_AUTO_JOIN_INVALID".into());
    }
    Ok(())
}
async fn ready(
    io: &impl Io,
    db: &Database,
    p: &PreparedManualSync,
    c: &Claim,
) -> Result<(), String> {
    let bytes = read(io, db, p, &c.ready_key().map_err(fail)?, LIMIT)
        .await?
        .ok_or("SYNC_AUTO_JOIN_NOT_READY")?;
    if Claim::parse(&bytes).map_err(fail)? != *c {
        return Err("SYNC_AUTO_JOIN_INVALID".into());
    }
    Ok(())
}

struct Bundle {
    documents: Vec<Option<String>>,
    terminals: lifecycle_sync::Document,
    plans: crate::daily_plan_protocol::Document,
    plans_present: bool,
    workflow: crate::task_workflow_protocol::Document,
    workflow_present: bool,
    terminals_present: bool,
}
async fn bundle(
    io: &impl Io,
    db: &Database,
    source: &PreparedManualSync,
    main: &str,
    required: bool,
) -> Result<Bundle, String> {
    let keys = crate::migration_backup::cloud::object_keys(main).map_err(fail)?;
    let mut documents = Vec::new();
    let mut total = 0;
    for (index, key) in keys.iter().enumerate() {
        let raw = read(io, db, source, key, LIMIT).await?;
        if required && raw.is_none() {
            return Err("SYNC_AUTO_JOIN_INCOMPLETE".into());
        }
        let text = raw
            .map(|raw| -> Result<String, String> {
                total += raw.len();
                if total > 64 * 1024 * 1024 {
                    return Err("SYNC_AUTO_JOIN_LIMIT".into());
                }
                let text = crate::sync_space::decode(key, std::str::from_utf8(&raw).map_err(fail)?)
                    .map_err(fail)?;
                crate::migration_backup::cloud::validate_document(index, text.as_bytes())
                    .map_err(fail)?;
                Ok(text)
            })
            .transpose()?;
        documents.push(text);
    }
    let target = source.retarget(main, source.epoch());
    let terminal_key = s3_sync::lifecycle_key(&target).map_err(fail)?;
    let raw = read(io, db, source, &terminal_key, 4 * 1024 * 1024).await?;
    if required && raw.is_none() {
        return Err("SYNC_AUTO_JOIN_INCOMPLETE".into());
    }
    let terminals_present = raw.is_some();
    let terminals = match raw {
        Some(raw) => lifecycle_sync::parse(
            &crate::sync_space::decode(&terminal_key, std::str::from_utf8(&raw).map_err(fail)?)
                .map_err(fail)?,
        )
        .map_err(fail)?,
        None => lifecycle_sync::Document {
            format_version: 1,
            terminals: vec![],
        },
    };
    let key = crate::daily_plan_sync::object_key(main, &keys).map_err(fail)?;
    let raw = read(io, db, source, &key, 4 * 1024 * 1024).await?;
    let plans_present = raw.is_some();
    let plans = raw
        .map(|b| {
            crate::daily_plan_protocol::parse(std::str::from_utf8(&b).map_err(fail)?).map_err(fail)
        })
        .transpose()?
        .unwrap_or_default();
    let mut occupied = keys;
    occupied.push(key);
    occupied.push(terminal_key);
    let key = crate::task_workflow_sync::object_key(main, &occupied).map_err(fail)?;
    let raw = read(io, db, source, &key, 4 * 1024 * 1024).await?;
    let workflow_present = raw.is_some();
    let workflow = raw
        .map(|b| {
            crate::task_workflow_protocol::parse(std::str::from_utf8(&b).map_err(fail)?)
                .map_err(fail)
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Bundle {
        documents,
        terminals,
        plans,
        plans_present,
        workflow,
        workflow_present,
        terminals_present,
    })
}

fn merge(tx: &Transaction<'_>, b: &Bundle, target_evidence: bool) -> Result<(), String> {
    let get = |i: usize| b.documents[i].as_deref();
    let parse = |text: &str| serde_json::from_str(text).map_err(fail);
    if let Some(s) = get(0) {
        crate::sync::merge_in_transaction(tx, &parse(s)?, now_millis())?;
    }
    if let Some(s) = get(1) {
        crate::note_sync::merge_in_transaction(
            tx,
            &serde_json::from_str(s).map_err(fail)?,
            now_millis(),
        )?;
    }
    if let Some(s) = get(2) {
        note_attachment_sync::merge_with_remote_evidence(
            tx,
            &serde_json::from_str(s).map_err(fail)?,
            now_millis(),
            target_evidence,
            true,
        )?;
    }
    if let Some(s) = get(3) {
        crate::recurrence_store::merge_in_transaction(
            tx,
            &crate::recurrence_protocol::parse_document(s)?,
        )?;
    }
    if let Some(s) = get(4) {
        crate::task_note_link_store::merge_in_transaction(
            tx,
            &crate::task_note_link_protocol::parse_document(s)?,
        )?;
    }
    let items = get(5)
        .map(crate::task_checklist_protocol::parse_items)
        .transpose()?
        .unwrap_or_default();
    let definitions = get(6)
        .map(crate::task_checklist_protocol::parse_definitions)
        .transpose()?
        .unwrap_or_default();
    crate::task_checklist_store::merge_in_transaction(tx, &items, &definitions)?;
    if let Some(s) = get(7) {
        crate::task_template_store::merge_in_transaction(
            tx,
            &crate::task_template_protocol::parse(s)?,
        )?;
    }
    crate::daily_plan_store::merge_in_transaction(tx, &b.plans)?;
    crate::task_workflow_store::merge_in_transaction(tx, &b.workflow)?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Asset {
    uuid: String,
    note: String,
    name: String,
    size: i64,
    hash: String,
    mime: String,
}
fn assets(c: &Connection) -> Result<Vec<Asset>, String> {
    let document = note_attachment_sync::build_backup_document(c, now_millis())?;
    let mut result = Vec::new();
    for a in document
        .attachments
        .iter()
        .filter(|a| a.deleted_at.is_none())
    {
        let referenced: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM notes WHERE uuid=?1)",
                [&a.note_uuid],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if !referenced {
            continue;
        }
        result.push(asset(a, "original")?);
        if a.kind == "image" {
            result.push(asset(a, "preview.jpg")?);
        }
    }
    result.sort();
    Ok(result)
}
fn asset(a: &SyncNoteAttachment, name: &str) -> Result<Asset, String> {
    let preview = name == "preview.jpg";
    Ok(Asset {
        uuid: a.uuid.clone(),
        note: a.note_uuid.clone(),
        name: name.into(),
        size: if preview {
            a.preview_byte_size.ok_or("SYNC_AUTO_JOIN_INVALID")?
        } else {
            a.byte_size
        },
        hash: if preview {
            a.preview_sha256.clone().ok_or("SYNC_AUTO_JOIN_INVALID")?
        } else {
            a.sha256.clone()
        },
        mime: if preview {
            "image/jpeg".into()
        } else {
            a.mime_type.clone()
        },
    })
}

// Used first as a rolled-back projection, then again with the latest local data when committing.
fn stage(
    tx: &Transaction<'_>,
    p: &PreparedManualSync,
    claim: &Claim,
    source: &Bundle,
    target: &Bundle,
) -> Result<String, String> {
    validate_binding(tx, p, claim)?;
    if !source.terminals_present {
        lifecycle_sync::require_remote(tx, p.epoch(), "")?;
    }
    if !source.plans_present
        && (crate::daily_plan_store::read_in_transaction(tx)?
            .etag
            .is_some()
            || crate::daily_plan_sync::remote_seen(tx, p.epoch())?)
    {
        return Err("SYNC_AUTO_JOIN_SOURCE_MISSING".into());
    }
    if !source.workflow_present
        && (crate::task_workflow_store::read_in_transaction(tx)?
            .etag
            .is_some()
            || crate::task_workflow_sync::remote_seen(tx, p.epoch())?)
    {
        return Err("SYNC_AUTO_JOIN_SOURCE_MISSING".into());
    }
    crate::sync_target::invalidate_in_transaction(tx)?;
    tx.execute(
        "UPDATE sync_settings SET object_key=?1 WHERE id=1",
        [claim.main().map_err(fail)?],
    )
    .map_err(db_error)?;
    crate::sync_target::activate(tx)?;
    space::record_auto_join_proof(tx, claim)?;
    let epoch = crate::sync_target::capture(tx)?;
    // Old-target upload flags are not evidence of any object in the new target.
    tx.execute("UPDATE note_attachments SET remote_uploaded=0", [])
        .map_err(db_error)?;
    let ledger = lifecycle_sync::merge(&source.terminals, &target.terminals)?;
    lifecycle_sync::prepare_in_transaction(tx, &epoch, &ledger)?;
    merge(tx, source, false)?;
    merge(tx, target, true)?;
    tx.execute("UPDATE note_attachments SET remote_uploaded=0", [])
        .map_err(db_error)?;
    tx.execute("DELETE FROM app_metadata WHERE key=?1", [space::PENDING])
        .map_err(db_error)?;
    Ok(epoch)
}

async fn ensure_asset(
    io: &impl Io,
    db: &Database,
    source: &PreparedManualSync,
    target: &PreparedManualSync,
    a: &Asset,
) -> Result<(), String> {
    guard(db, source)?;
    let head = io.head(target, a).await?;
    guard(db, source)?;
    if head.exists {
        if head.content_length != Some(a.size) || head.sha256.as_deref() != Some(a.hash.as_str()) {
            return Err("SYNC_AUTO_JOIN_ASSET_MISMATCH".into());
        }
        return Ok(());
    }
    let bytes = match io.local(a) {
        Ok(bytes) => bytes,
        Err(_) => io.download(source, a).await?,
    };
    guard(db, source)?;
    if bytes.len() as i64 != a.size || space::hash(&bytes) != a.hash {
        return Err("SYNC_AUTO_JOIN_ASSET_MISMATCH".into());
    }
    io.upload(target, a, &bytes).await?;
    guard(db, source)?;
    let verified = io.head(target, a).await?;
    guard(db, source)?;
    if !verified.exists
        || verified.content_length != Some(a.size)
        || verified.sha256.as_deref() != Some(a.hash.as_str())
    {
        return Err("SYNC_AUTO_JOIN_ASSET_MISMATCH".into());
    }
    Ok(())
}

async fn follow_with(
    io: &impl Io,
    db: &Database,
    p: &PreparedManualSync,
) -> Result<Option<PreparedManualSync>, String> {
    let Some(claim) = association(io, db, p, true).await? else {
        return Ok(None);
    };
    let main = claim.main().map_err(fail)?;
    let source = bundle(io, db, p, p.main_key(), false).await?;
    let target = bundle(io, db, p, &main, true).await?;
    let needed = {
        let mut c = lock_database(db)?;
        let tx = c.transaction().map_err(db_error)?;
        stage(&tx, p, &claim, &source, &target)?;
        assets(&tx)?
        // Rollback: nothing has changed in the real configuration or local data yet.
    };
    let destination = p.retarget(&main, p.epoch());
    for a in &needed {
        ensure_asset(io, db, p, &destination, a).await?;
    }
    if association(io, db, p, false).await?.as_ref() != Some(&claim) {
        return Err("SYNC_AUTO_JOIN_ASSOCIATION_CHANGED".into());
    }
    // Re-read the target after transfers; a concurrent purge must win before committing.
    let refreshed_target = bundle(io, db, p, &main, true).await?;
    if target.workflow_present && !refreshed_target.workflow_present {
        return Err("SYNC_AUTO_JOIN_SOURCE_MISSING".into());
    }
    let target = refreshed_target;
    let mut c = lock_database(db)?;
    let tx = c.transaction().map_err(db_error)?;
    let epoch = stage(&tx, p, &claim, &source, &target)?;
    let current = assets(&tx)?;
    if current.iter().any(|a| !needed.contains(a)) {
        return Err("SYNC_AUTO_JOIN_CHANGED_RETRY".into());
    }
    for a in current.iter().filter(|a| a.name == "original") {
        tx.execute("UPDATE note_attachments SET remote_uploaded=1,transfer_state=CASE WHEN local_original_path IS NULL THEN 'remote_only' ELSE 'synced' END,transfer_error=NULL WHERE uuid=?1", [&a.uuid]).map_err(db_error)?;
    }
    tx.commit().map_err(db_error)?;
    Ok(Some(p.retarget(&main, &epoch)))
}
pub(crate) async fn follow(
    db: &Database,
    runtime: &SyncRuntime,
    assets: &NoteAssetStore,
    p: &PreparedManualSync,
) -> Result<Option<PreparedManualSync>, String> {
    follow_with(
        &Live {
            runtime,
            assets: Some(assets),
        },
        db,
        p,
    )
    .await
    .map_err(|e| {
        if e.starts_with("SYNC_AUTO_JOIN_") {
            e
        } else {
            "SYNC_AUTO_JOIN_FAILED".into()
        }
    })
}
pub(crate) async fn probe(db: &Database, p: &PreparedManualSync) -> Result<bool, String> {
    let runtime = SyncRuntime::default();
    let io = Live {
        runtime: &runtime,
        assets: None,
    };
    let Some(claim) = association(&io, db, p, false).await? else {
        return Ok(false);
    };
    bundle(&io, db, p, &claim.main().map_err(fail)?, true).await?;
    Ok(true)
}

#[cfg(test)]
#[path = "sync_auto_join_tests.rs"]
mod tests;
