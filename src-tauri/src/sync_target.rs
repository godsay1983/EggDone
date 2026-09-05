//! Local configuration epoch. It contains no target URL or credentials and is never synchronized.
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const KEY: &str = "sync.target.epoch.v1";

pub(crate) fn capture(connection: &Connection) -> Result<String, String> {
    connection
        .execute(
            "INSERT OR IGNORE INTO app_metadata(key,value) VALUES(?1,?2)",
            params![KEY, Uuid::new_v4().to_string()],
        )
        .map_err(|_| "SYNC_TARGET_DATABASE")?;
    let epoch: String = connection
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .map_err(|_| "SYNC_TARGET_DATABASE")?;
    if epoch.starts_with("pending:") {
        return Err("SYNC_TARGET_SAVE_INCOMPLETE".into());
    }
    Ok(epoch)
}

pub(crate) fn activate(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "UPDATE app_metadata SET value=?1 WHERE key=?2 AND value LIKE 'pending:%'",
            params![Uuid::new_v4().to_string(), KEY],
        )
        .map_err(|_| "SYNC_TARGET_DATABASE")?;
    Ok(())
}

pub(crate) fn is_current(connection: &Connection, epoch: &str) -> Result<bool, String> {
    let current: Option<String> = connection
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|_| "SYNC_TARGET_DATABASE")?;
    Ok(current.as_deref() == Some(epoch))
}

// Call before changing the external credential store, while holding the application's DB mutex.
// Failed credential writes must still invalidate in-flight work. No rollback restores an old epoch.
pub(crate) fn invalidate(connection: &Connection) -> Result<(), String> {
    let tx = connection
        .unchecked_transaction()
        .map_err(|_| "SYNC_TARGET_DATABASE")?;
    tx.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![KEY,format!("pending:{}", Uuid::new_v4())]).map_err(|_| "SYNC_TARGET_DATABASE")?;
    let changed=tx.execute("UPDATE sync_runtime_state SET
        todos_dirty_version=todos_dirty_version+1,notes_dirty_version=notes_dirty_version+1,
        attachments_dirty_version=attachments_dirty_version+1,dirty_domains='[\"todos\",\"notes\",\"attachments\"]',
        dirty_since=COALESCE(dirty_since,?1),last_success_at=NULL,last_result='interrupted',
        last_error_code=NULL,last_error_message=NULL,updated_at=?1
        WHERE id=1 AND todos_dirty_version<9007199254740991 AND notes_dirty_version<9007199254740991
        AND attachments_dirty_version<9007199254740991",[crate::db::now_millis()]).map_err(|_| "SYNC_TARGET_DATABASE")?;
    if changed != 1 {
        return Err("SYNC_TARGET_REVISION_LIMIT".into());
    }
    if tx.execute("UPDATE recurrence_sync_state SET revision=revision+1,etag=NULL WHERE id=1 AND revision<9007199254740991",[])
        .map_err(|_| "SYNC_TARGET_DATABASE")? != 1 { return Err("SYNC_TARGET_REVISION_LIMIT".into()); }
    tx.commit().map_err(|_| "SYNC_TARGET_DATABASE".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_back_never_reuses_epoch_and_all_domains_stay_dirty() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut db).unwrap();
        let first = capture(&db).unwrap();
        assert_eq!(capture(&db).unwrap(), first);
        invalidate(&db).unwrap();
        assert_eq!(capture(&db).unwrap_err(), "SYNC_TARGET_SAVE_INCOMPLETE");
        activate(&db).unwrap();
        let second = capture(&db).unwrap();
        invalidate(&db).unwrap();
        activate(&db).unwrap();
        let third = capture(&db).unwrap();
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert!(!is_current(&db, &first).unwrap());
        let (todos,notes,assets,dirty):(i64,i64,i64,String)=db.query_row("SELECT todos_dirty_version,notes_dirty_version,attachments_dirty_version,dirty_domains FROM sync_runtime_state",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!((todos, notes, assets), (2, 2, 2));
        assert_eq!(dirty, "[\"todos\",\"notes\",\"attachments\"]");
        assert!(!crate::sync_runtime_state::mark_domain_synced(
            &db,
            crate::sync_runtime_state::SyncDomain::Todos,
            0
        )
        .unwrap());
    }
    #[test]
    fn invalidation_failure_rolls_back_epoch_and_versions() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut db).unwrap();
        let epoch = capture(&db).unwrap();
        db.execute(
            "UPDATE recurrence_sync_state SET revision=9007199254740991",
            [],
        )
        .unwrap();
        assert_eq!(invalidate(&db).unwrap_err(), "SYNC_TARGET_REVISION_LIMIT");
        assert!(is_current(&db, &epoch).unwrap());
        assert_eq!(
            db.query_row(
                "SELECT todos_dirty_version FROM sync_runtime_state",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
