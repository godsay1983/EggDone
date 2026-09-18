//! Durable, target-bound attachment cleanup after terminal and body publication.
use crate::{
    db::Database,
    lifecycle_sync,
    note_attachment_sync::SyncNoteAttachment,
    s3_sync::{self, PreparedManualSync, SyncRuntime},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PREFIX: &str = "purge.remote.evidence.v1:";
const DONE: &str = "purge.remote.done.v1:";
const TARGET: &str = "purge.remote.target.v1:";
pub(crate) fn capture_local(c: &Connection, note: &str) -> Result<(), String> {
    let main: String = c
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(|_| "PURGE_DATABASE_FAILED")?;
    if crate::sync_space::scope(&main)?.is_none() {
        return Ok(());
    }
    let epoch = crate::sync_target::capture(c)?;
    bind_target(c, &epoch)?;
    let mut q = c.prepare("SELECT uuid,kind,byte_size,sha256,preview_byte_size,preview_sha256 FROM note_attachments WHERE note_uuid=?1 AND remote_uploaded=1").map_err(|_| "PURGE_DATABASE_FAILED")?;
    let rows = q
        .query_map([note], |r| {
            Ok(Evidence(
                1,
                epoch.clone(),
                main.clone(),
                note.into(),
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .map_err(|_| "PURGE_DATABASE_FAILED")?;
    for row in rows {
        let evidence = row.map_err(|_| "PURGE_DATABASE_FAILED")?;
        evidence.validate()?;
        c.execute(
            "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,?2)",
            params![
                format!("{PREFIX}{epoch}:{}", evidence.4),
                serde_json::to_string(&evidence).map_err(|_| "PURGE_REMOTE_INVALID")?
            ],
        )
        .map_err(|_| "PURGE_DATABASE_FAILED")?;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// Stable v1 wire: version, epoch, main key, note/asset UUID, kind, original size/hash, preview size/hash.
pub(crate) struct Evidence(
    u32,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    Option<i64>,
    Option<String>,
);
impl Evidence {
    fn validate(&self) -> Result<(), String> {
        let valid_id = |id: &str| uuid::Uuid::parse_str(id).is_ok();
        let sha = |v: &str| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.0 != 1
            || self.1.is_empty()
            || self.1.starts_with("pending:")
            || !valid_id(&self.3)
            || !valid_id(&self.4)
            || !matches!(crate::sync_space::scope(&self.2)?, Some((_, "todos")))
            || !["image", "file"].contains(&self.5.as_str())
            || !(1..=8 * 1024 * 1024 * 1024).contains(&self.6)
            || !sha(&self.7)
            || (self.5 == "image"
                && (!self.8.is_some_and(|n| (1..=2 * 1024 * 1024).contains(&n))
                    || !self.9.as_deref().is_some_and(sha)))
            || (self.5 == "file" && (self.8.is_some() || self.9.is_some()))
        {
            return Err("PURGE_REMOTE_EVIDENCE_INVALID".into());
        }
        Ok(())
    }
    fn raw(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| "PURGE_REMOTE_EVIDENCE_INVALID".into())
    }
    fn key(&self, prefix: &str) -> String {
        format!("{prefix}{}:{}", self.1, self.4)
    }
    pub(crate) fn from_asset(epoch: &str, key: &str, a: &SyncNoteAttachment) -> Self {
        Self(
            1,
            epoch.into(),
            key.into(),
            a.note_uuid.clone(),
            a.uuid.clone(),
            a.kind.clone(),
            a.byte_size,
            a.sha256.clone(),
            a.preview_byte_size,
            a.preview_sha256.clone(),
        )
    }
}
fn db(_: rusqlite::Error) -> String {
    "PURGE_DATABASE_FAILED".into()
}
fn value(c: &Connection, key: &str) -> Result<Option<String>, String> {
    c.query_row("SELECT value FROM app_metadata WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(db)
}
// Credentials and enabled state do not change the storage identity. Never retarget by main key alone.
pub(crate) fn bind_target(c: &Connection, epoch: &str) -> Result<String, String> {
    if epoch.is_empty() || epoch.starts_with("pending:") {
        return Err("PURGE_TARGET_CHANGED".into());
    }
    let values = c.query_row("SELECT endpoint,region,bucket,object_key,path_style,allow_http FROM sync_settings WHERE id=1", [], |r| {
        Ok(serde_json::json!([r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?,r.get::<_,bool>(5)?]))
    }).map_err(db)?;
    let binding = format!("{:x}", Sha256::digest(values.to_string().as_bytes()));
    let key = format!("{TARGET}{epoch}");
    if value(c, &key)?.is_some_and(|old| old != binding) {
        return Err("PURGE_REMOTE_TARGET_CONFLICT".into());
    }
    c.execute(
        "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,?2)",
        params![key, binding],
    )
    .map_err(db)?;
    Ok(binding)
}
pub(crate) fn save(c: &Connection, evidence: &Evidence) -> Result<(), String> {
    let raw = evidence.raw()?;
    let key = evidence.key(PREFIX);
    let old: Option<String> = c
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db)?;
    if let Some(old) = old {
        let parsed: Evidence =
            serde_json::from_str(&old).map_err(|_| "PURGE_REMOTE_EVIDENCE_INVALID")?;
        if &parsed != evidence {
            return Err("PURGE_REMOTE_EVIDENCE_CONFLICT".into());
        }
    } else {
        c.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![key, raw],
        )
        .map_err(db)?;
    }
    Ok(())
}
// The caller owns the body merge transaction. Capture remote-only files before filtering metadata.
pub(crate) fn capture(c: &Connection, attachments: &[SyncNoteAttachment]) -> Result<(), String> {
    let key: String = c
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(db)?;
    if crate::sync_space::scope(&key)?.is_none() {
        return Ok(());
    }
    let epoch: String = c
        .query_row(
            "SELECT value FROM app_metadata WHERE key='sync.target.epoch.v1'",
            [],
            |r| r.get(0),
        )
        .map_err(db)?;
    lifecycle_sync::guard(c, &epoch)?;
    bind_target(c, &epoch)?;
    let index = lifecycle_sync::Index::read(c)?;
    for a in attachments {
        if !index.note(&a.note_uuid) {
            continue;
        }
        save(c, &Evidence::from_asset(&epoch, &key, a))?;
        // A late writer may reintroduce metadata/binary after an earlier successful cleanup.
        c.execute(
            "DELETE FROM app_metadata WHERE key=?1",
            [format!("{DONE}{epoch}:{}", a.uuid)],
        )
        .map_err(db)?;
        c.execute("INSERT OR IGNORE INTO purge_cleanup(attachment_uuid,note_uuid,local_done) VALUES(?1,?2,1)",params![a.uuid,a.note_uuid]).map_err(db)?;
        c.execute(
            "UPDATE purge_cleanup SET remote_done=0 WHERE attachment_uuid=?1",
            [&a.uuid],
        )
        .map_err(db)?;
    }
    Ok(())
}
fn pending(c: &Connection, epoch: &str) -> Result<Vec<Evidence>, String> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate).map_err(db)?;
    let jobs = pending_in_transaction(&tx, epoch)?;
    tx.commit().map_err(db)?;
    Ok(jobs)
}
fn pending_in_transaction(c: &Connection, epoch: &str) -> Result<Vec<Evidence>, String> {
    let key = lifecycle_sync::guard(c, epoch)?;
    let binding = bind_target(c, epoch)?;
    let saved = {
        let mut q = c
            .prepare(
                "SELECT key,value FROM app_metadata WHERE substr(key,1,length(?1))=?1 ORDER BY key",
            )
            .map_err(db)?;
        let rows = q
            .query_map([PREFIX], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(db)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db)?
    };
    // Move only unfinished evidence for this exact storage target into the new local session epoch.
    for (stored_key, raw) in saved {
        let old: Evidence =
            serde_json::from_str(&raw).map_err(|_| "PURGE_REMOTE_EVIDENCE_INVALID")?;
        old.validate()?;
        if stored_key != old.key(PREFIX) {
            return Err("PURGE_REMOTE_EVIDENCE_CONFLICT".into());
        }
        if old.1 == epoch || value(c, &old.key(DONE))?.as_deref() == Some(raw.as_str()) {
            continue;
        }
        let previous =
            value(c, &format!("{TARGET}{}", old.1))?.ok_or("PURGE_REMOTE_TARGET_UNKNOWN")?;
        if previous.len() != 64
            || !previous
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("PURGE_REMOTE_TARGET_UNKNOWN".into());
        }
        if previous != binding {
            continue;
        }
        if old.2 != key {
            return Err("PURGE_TARGET_CHANGED".into());
        }
        let mut current = old.clone();
        current.1 = epoch.into();
        save(c, &current)?;
        c.execute("DELETE FROM app_metadata WHERE key=?1", [current.key(DONE)])
            .map_err(db)?;
        c.execute(
            "UPDATE purge_cleanup SET remote_done=0 WHERE attachment_uuid=?1",
            [&current.4],
        )
        .map_err(db)?;
        c.execute(
            "DELETE FROM app_metadata WHERE key=?1 OR key=?2",
            params![old.key(PREFIX), old.key(DONE)],
        )
        .map_err(db)?;
    }
    let prefix = format!("{PREFIX}{epoch}:");
    let mut q = c
        .prepare("SELECT value FROM app_metadata WHERE substr(key,1,length(?1))=?1 ORDER BY key")
        .map_err(db)?;
    let mut jobs = vec![];
    for row in q
        .query_map([prefix], |r| r.get::<_, String>(0))
        .map_err(db)?
    {
        let raw = row.map_err(db)?;
        let evidence: Evidence =
            serde_json::from_str(&raw).map_err(|_| "PURGE_REMOTE_EVIDENCE_INVALID")?;
        evidence.validate()?;
        if evidence.1 != epoch || evidence.2 != key {
            return Err("PURGE_TARGET_CHANGED".into());
        }
        let done: Option<String> = c
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [evidence.key(DONE)],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        if done.as_deref() != Some(raw.as_str()) {
            jobs.push(evidence);
        }
    }
    Ok(jobs)
}
fn require_local(c: &Connection, job: &Evidence, token: &str) -> Result<(), String> {
    if lifecycle_sync::guard(c, &job.1)? != job.2 {
        return Err("PURGE_TARGET_CHANGED".into());
    }
    let ready:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM lifecycle_sync_state WHERE id=1 AND revision=synced_revision AND etag=?1) AND EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=?2) AND NOT EXISTS(SELECT 1 FROM notes WHERE uuid=?2) AND NOT EXISTS(SELECT 1 FROM note_attachments WHERE uuid=?3)",params![token,job.3,job.4],|r|r.get(0)).map_err(db)?;
    if !ready {
        return Err("PURGE_REMOTE_NOT_READY".into());
    }
    let raw: String = c
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [job.key(PREFIX)],
            |r| r.get(0),
        )
        .map_err(db)?;
    if serde_json::from_str::<Evidence>(&raw).map_err(|_| "PURGE_REMOTE_EVIDENCE_INVALID")? != *job
    {
        return Err("PURGE_REMOTE_EVIDENCE_CONFLICT".into());
    }
    Ok(())
}
fn acknowledge(c: &mut Connection, job: &Evidence, token: &str) -> Result<(), String> {
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    require_local(&tx, job, token)?;
    tx.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![job.key(DONE),job.raw()?]).map_err(db)?;
    tx.execute(
        "UPDATE purge_cleanup SET remote_done=1 WHERE attachment_uuid=?1 AND note_uuid=?2",
        params![job.4, job.3],
    )
    .map_err(db)?;
    tx.commit().map_err(db)
}
async fn verify(
    db: &Database,
    prepared: &PreparedManualSync,
    job: &Evidence,
    token: &str,
) -> Result<(), String> {
    {
        let c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
        prepared.require_current(&c)?;
        require_local(&c, job, token)?;
    }
    let (ledger, current) = s3_sync::download_lifecycle(prepared).await?;
    if current != token
        || !ledger
            .terminals
            .iter()
            .any(|t| t.kind == "note" && t.uuid == job.3)
    {
        return Err("PURGE_LEDGER_CHANGED_RETRY".into());
    }
    let metadata = s3_sync::download_note_attachment_remote(prepared).await?;
    let metadata = metadata.document.ok_or("PURGE_REMOTE_NOT_READY")?;
    if metadata.attachments.iter().any(|a| a.uuid == job.4) {
        return Err("PURGE_REMOTE_REFERENCED".into());
    }
    crate::lifecycle_session::guard(db, prepared)
}
pub(crate) async fn run(
    db: &Database,
    runtime: &SyncRuntime,
    prepared: &PreparedManualSync,
    token: &str,
) -> Result<usize, String> {
    let jobs = {
        let c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
        pending(&c, prepared.epoch())?
    };
    let mut deleted = 0;
    for job in jobs {
        verify(db, prepared, &job, token).await?;
        let guard = || {
            let c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
            prepared.require_current(&c)?;
            require_local(&c, &job, token)
        };
        if s3_sync::delete_asset_if_matches_guarded(
            runtime, prepared, &job.4, "original", job.6, &job.7, &guard,
        )
        .await?
        {
            deleted += 1;
        }
        if job.5 == "image" {
            verify(db, prepared, &job, token).await?;
            if s3_sync::delete_asset_if_matches_guarded(
                runtime,
                prepared,
                &job.4,
                "preview.jpg",
                job.8.ok_or("PURGE_REMOTE_EVIDENCE_INVALID")?,
                job.9.as_deref().ok_or("PURGE_REMOTE_EVIDENCE_INVALID")?,
                &guard,
            )
            .await?
            {
                deleted += 1;
            }
        }
        let mut c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
        acknowledge(&mut c, &job, token)?;
    }
    Ok(deleted)
}

#[cfg(test)]
#[path = "purge_remote_tests.rs"]
mod tests;
