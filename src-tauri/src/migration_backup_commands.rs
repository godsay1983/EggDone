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
            let mut source = None;
            backup::copy_with_missing(&work, &root, &app_data.join("note-assets"), |entry| {
                {
                    let connection = lock_database(&db)?;
                    backup::require_current(&connection, &work.plan)?;
                    if source.is_none() {
                        source = Some(crate::s3_sync::prepare_migration_asset_source(&connection)?);
                    }
                }
                let bytes = tauri::async_runtime::block_on(
                    source
                        .as_ref()
                        .ok_or("MIGRATION_BACKUP_ASSET_CONFIG")?
                        .download(
                            &runtime,
                            &entry.name[..36],
                            &entry.name[37..],
                            entry.size as i64,
                            &entry.sha256,
                        ),
                )?;
                backup::require_current(&*lock_database(&db)?, &work.plan)?;
                Ok(bytes)
            })?;
            backup::rehearse(&root, &work.plan)?;
            return backup::finish(&mut *lock_database(&db)?, &work.plan, now_millis()).map(Some);
        }
        let plan = backup::latest(&*lock_database(&db)?)?;
        let Some(plan) = plan else {
            return Ok(None);
        };
        if action == "prepareCloud" {
            if plan.verified_at.is_none() {
                return Err("MIGRATION_CLOUD_LOCAL_REQUIRED".into());
            }
            backup::require_current(&*lock_database(&db)?, &plan)?;
            backup::verify_files(&root, &plan)?;
            backup::rehearse(&root, &plan)?;
            let (source, binding) = {
                let connection = lock_database(&db)?;
                backup::require_current(&connection, &plan)?;
                (
                    crate::s3_sync::prepare_migration_asset_source(&connection)?,
                    crate::s3_sync::migration_source_binding(&connection)?,
                )
            };
            let remote = tauri::async_runtime::block_on(source.metadata())?;
            let cloud = backup::cloud::prepare(
                &mut *lock_database(&db)?,
                &plan,
                &binding,
                source.main_key(),
                &remote,
                now_millis(),
            )?;
            backup::cloud::copy(&root, &plan, &cloud, &remote, |entry| {
                backup::cloud::require_current(&*lock_database(&db)?, &plan, &cloud)?;
                let bytes = tauri::async_runtime::block_on(source.download(
                    &runtime,
                    &entry.name[..36],
                    &entry.name[37..],
                    entry.size as i64,
                    &entry.sha256,
                ))?;
                backup::cloud::require_current(&*lock_database(&db)?, &plan, &cloud)?;
                Ok(bytes)
            })?;
            let final_remote = tauri::async_runtime::block_on(source.metadata())?;
            backup::cloud::require_remote(&cloud, &final_remote)?;
            backup::verify_files(&root, &plan)?;
            backup::cloud::verify_files(&root, &plan, &cloud)?;
            backup::cloud::finish(&mut *lock_database(&db)?, &plan, &cloud, now_millis())?;
            backup::report(&mut *lock_database(&db)?, &plan).map(Some)
        } else if action == "verify" {
            backup::verify_files(&root, &plan)?;
            backup::rehearse(&root, &plan)?;
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
