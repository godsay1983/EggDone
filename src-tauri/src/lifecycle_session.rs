use crate::{
    db::Database,
    lifecycle_sync as ledger,
    s3_sync::{self, PreparedManualSync, UploadOutcome},
};

// The owner holds SyncRuntime for the entire ledger + entity session. The returned token must be rechecked at completion.
pub(crate) async fn run(db: &Database, prepared: &PreparedManualSync) -> Result<String, String> {
    for _ in 0..3 {
        guard(db, prepared)?;
        let (remote, etag) = s3_sync::download_lifecycle(prepared).await?;
        let snapshot = {
            let mut c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
            prepared.require_current(&c)?;
            ledger::prepare(&mut c, prepared.epoch(), &remote)?
        };
        guard(db, prepared)?;
        if s3_sync::upload_lifecycle(prepared, &snapshot.document, &etag).await?
            == UploadOutcome::Conflict
        {
            continue;
        }
        guard(db, prepared)?;
        let (verified, token) = s3_sync::download_lifecycle(prepared).await?;
        // Do not ACK an overwritten/truncated ledger or skip concurrent deletion evidence.
        if ledger::merge(&snapshot.document, &verified)? != snapshot.document
            || ledger::merge(&verified, &verified)? != snapshot.document
        {
            continue;
        }
        let mut c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
        prepared.require_current(&c)?;
        if ledger::acknowledge(&mut c, prepared.epoch(), snapshot.revision, &token)? {
            return Ok(token);
        }
    }
    Err("PURGE_LEDGER_CONFLICT".into())
}
pub(crate) fn guard(db: &Database, prepared: &PreparedManualSync) -> Result<(), String> {
    let c = db.connection.lock().map_err(|_| "PURGE_DATABASE_FAILED")?;
    prepared.require_current(&c)
}
