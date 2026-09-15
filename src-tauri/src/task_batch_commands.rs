use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_batch::{self, BatchResult},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn create_task_batch(
    database: State<'_, Database>,
    app: AppHandle,
    request: serde_json::Value,
) -> Result<BatchResult, String> {
    let request = task_batch::parse(&request.to_string())?;
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_batch::prepare(&mut db, &request)?;
        task_batch::create(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn load_task_batch_recovery(
    database: State<'_, Database>,
) -> Result<Option<task_batch::BatchRequest>, String> {
    let db = lock_database(&database)?;
    task_batch::pending(&db)
}

#[tauri::command]
pub fn forget_task_batch_recovery(
    database: State<'_, Database>,
    request: serde_json::Value,
) -> Result<(), String> {
    let request = task_batch::parse(&request.to_string())?;
    let mut db = lock_database(&database)?;
    task_batch::forget(&mut db, &request)
}
