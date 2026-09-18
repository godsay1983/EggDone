use crate::{
    commands::lock_database,
    db::{now_millis, Database},
    note_asset_store::NoteAssetStore,
    purge::{self, Plan, Target},
    s3_sync::SyncRuntime,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub async fn prepare_trash_purge(
    app: AppHandle,
    selected: Option<Vec<Target>>,
) -> Result<Plan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<Database>();
        let runtime = app.state::<SyncRuntime>();
        let _guard = runtime.acquire()?;
        let mut connection = lock_database(&database)?;
        purge::prepare(&mut connection, selected, now_millis())
    })
    .await
    .map_err(|_| "PURGE_WORKER_FAILED".to_string())?
}

#[tauri::command]
pub async fn run_trash_purge(app: AppHandle, operation_uuid: String) -> Result<Plan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<Database>();
        let runtime = app.state::<SyncRuntime>();
        let assets = app.state::<NoteAssetStore>();
        let _guard = runtime.acquire()?;
        let result = {
            let mut connection = lock_database(&database)?;
            purge::execute_batch(&mut connection, &operation_uuid, now_millis())?;
            // A failed file cleanup does not turn an already committed deletion into a failed write.
            let _ = purge::cleanup(&connection, &assets, &operation_uuid);
            purge::status(&connection, &operation_uuid)?
        };
        crate::tray::update_task_badge(&app);
        let _ = app.emit_to("main", "todos-changed", ());
        let _ = app.emit_to("main", "notes-changed", ());
        Ok(result)
    })
    .await
    .map_err(|_| "PURGE_WORKER_FAILED".to_string())?
}

#[tauri::command]
pub fn unfinished_trash_purge(database: State<'_, Database>) -> Result<Option<Plan>, String> {
    let connection = lock_database(&database)?;
    purge::unfinished(&connection)
}
