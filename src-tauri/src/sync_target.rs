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
    invalidate_in_transaction(&tx)?;
    tx.commit().map_err(|_| "SYNC_TARGET_DATABASE".into())
}

pub(crate) fn invalidate_in_transaction(tx: &Connection) -> Result<(), String> {
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
    if tx.execute("UPDATE task_note_link_sync_state SET revision=revision+1,etag=NULL WHERE id=1 AND revision<9007199254740991",[])
        .map_err(|_| "SYNC_TARGET_DATABASE")? != 1 { return Err("SYNC_TARGET_REVISION_LIMIT".into()); }
    if tx.execute("UPDATE task_checklist_sync_state SET revision=revision+1,generation=generation+1,etag=NULL
        WHERE revision<9007199254740991 AND generation<9007199254740991",[])
        .map_err(|_| "SYNC_TARGET_DATABASE")? != 2 { return Err("SYNC_TARGET_REVISION_LIMIT".into()); }
    if tx.execute("UPDATE task_template_sync_state SET revision=revision+1,generation=generation+1,etag=NULL
        WHERE id=1 AND revision<9007199254740991 AND generation<9007199254740991",[])
        .map_err(|_| "SYNC_TARGET_DATABASE")? != 1 { return Err("SYNC_TARGET_REVISION_LIMIT".into()); }
    if tx.execute("UPDATE lifecycle_sync_state SET revision=revision+1,etag=NULL WHERE id=1 AND revision<9007199254740991",[])
        .map_err(|_| "SYNC_TARGET_DATABASE")? != 1 { return Err("SYNC_TARGET_REVISION_LIMIT".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(db: &Connection, sql: &str) -> Vec<Vec<rusqlite::types::Value>> {
        let mut query = db.prepare(sql).unwrap();
        let count = query.column_count();
        query
            .query_map([], |row| (0..count).map(|i| row.get(i)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn seed_lifecycle_ack(db: &Connection, epoch: &str, revision: i64) {
        db.execute_batch(
            "INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at)
             VALUES('note','123e4567-e89b-42d3-a456-426614174001','123e4567-e89b-42d3-a456-426614174090',200);
             INSERT INTO purge_cleanup(attachment_uuid,note_uuid,original_path,local_done,local_attempts,remote_done)
             VALUES('123e4567-e89b-42d3-a456-426614174002','123e4567-e89b-42d3-a456-426614174001','old/file',1,2,1);",
        ).unwrap();
        let evidence = serde_json::json!([
            1,
            epoch,
            "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/todos.json",
            "123e4567-e89b-42d3-a456-426614174001",
            "123e4567-e89b-42d3-a456-426614174002",
            "file",
            10,
            "a".repeat(64),
            null,
            null
        ])
        .to_string();
        for prefix in ["purge.remote.evidence.v1:", "purge.remote.done.v1:"] {
            db.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![
                    format!("{prefix}{epoch}:123e4567-e89b-42d3-a456-426614174002"),
                    evidence
                ],
            )
            .unwrap();
        }
        db.execute(
            "UPDATE lifecycle_sync_state SET revision=?1,synced_revision=?1,etag='\"old-ledger\"' WHERE id=1",
            [revision],
        ).unwrap();
    }

    #[test]
    fn invalidation_clears_lifecycle_ack_without_changing_terminal_or_cleanup_evidence() {
        for revision in [7, 9007199254740990] {
            let mut db = Connection::open_in_memory().unwrap();
            crate::db::migrate(&mut db).unwrap();
            let epoch = capture(&db).unwrap();
            seed_lifecycle_ack(&db, &epoch, revision);
            let queries = [
                "SELECT * FROM lifecycle_terminals ORDER BY kind,uuid",
                "SELECT * FROM purge_cleanup ORDER BY attachment_uuid",
                "SELECT * FROM app_metadata WHERE key LIKE 'purge.remote.%' ORDER BY key",
            ];
            let before: Vec<_> = queries.iter().map(|sql| rows(&db, sql)).collect();
            invalidate(&db).unwrap();
            assert!(!is_current(&db, &epoch).unwrap());
            assert_eq!(capture(&db).unwrap_err(), "SYNC_TARGET_SAVE_INCOMPLETE");
            let state: (i64, i64, Option<String>) = db
                .query_row(
                    "SELECT revision,synced_revision,etag FROM lifecycle_sync_state WHERE id=1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(state, (revision + 1, revision, None));
            assert_eq!(
                queries.iter().map(|sql| rows(&db, sql)).collect::<Vec<_>>(),
                before
            );
        }
    }

    #[test]
    fn lifecycle_revision_overflow_rolls_back_epoch_and_all_domains() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut db).unwrap();
        let epoch = capture(&db).unwrap();
        seed_lifecycle_ack(&db, &epoch, 9007199254740991);
        let queries = [
            "SELECT * FROM app_metadata ORDER BY key",
            "SELECT * FROM sync_runtime_state",
            "SELECT * FROM recurrence_sync_state",
            "SELECT * FROM task_note_link_sync_state",
            "SELECT * FROM task_checklist_sync_state ORDER BY 1",
            "SELECT * FROM task_template_sync_state",
            "SELECT * FROM lifecycle_sync_state",
            "SELECT * FROM lifecycle_terminals ORDER BY kind,uuid",
            "SELECT * FROM purge_cleanup ORDER BY attachment_uuid",
        ];
        let before: Vec<_> = queries.iter().map(|sql| rows(&db, sql)).collect();
        assert_eq!(invalidate(&db).unwrap_err(), "SYNC_TARGET_REVISION_LIMIT");
        assert!(is_current(&db, &epoch).unwrap());
        assert_eq!(
            queries.iter().map(|sql| rows(&db, sql)).collect::<Vec<_>>(),
            before
        );
    }

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
