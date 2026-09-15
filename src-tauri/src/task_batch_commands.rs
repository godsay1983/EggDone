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
        task_batch::create(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}
