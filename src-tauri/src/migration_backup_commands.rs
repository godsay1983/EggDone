use crate::{
    commands::lock_database,
    db::{now_millis, Database},
    migration_backup as backup,
    s3_sync::SyncRuntime,
};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn migration_local_backup(
    app: AppHandle,
    action: String,
) -> Result<Option<backup::BackupReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Database>();
        let runtime = app.state::<SyncRuntime>();
        let _guard = runtime.acquire().map_err(|_| "MIGRATION_SYNC_BUSY")?;
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|_| "MIGRATION_BACKUP_IO")?;
        let root = app_data.join("migration-backups");
        if action == "prepare" {
            let work = backup::prepare(&mut *lock_database(&db)?, now_millis())?;
            // Do not keep the database locked while copying files; finish rejects concurrent edits.
            backup::copy(&work, &root, &app_data.join("note-assets"))?;
            return backup::finish(&mut *lock_database(&db)?, &work.plan, now_millis()).map(Some);
        }
        let plan = backup::latest(&*lock_database(&db)?)?;
        let Some(plan) = plan else {
            return Ok(None);
        };
        if action == "verify" {
            backup::verify_files(&root, &plan)?;
            backup::finish(&mut *lock_database(&db)?, &plan, now_millis()).map(Some)
        } else if action == "status" {
            backup::report(&mut *lock_database(&db)?, &plan).map(Some)
        } else {
            Err("MIGRATION_BACKUP_INVALID".into())
        }
    })
    .await
    .map_err(|_| "MIGRATION_BACKUP_WORKER".to_string())?
}
