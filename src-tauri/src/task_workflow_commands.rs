use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_workflow_store::{self, WorkflowSnapshot, WorkflowWrite},
};
use tauri::{AppHandle, Emitter, State};
#[tauri::command]
pub fn list_task_workflow(
    database: State<'_, Database>,
    date: String,
) -> Result<WorkflowSnapshot, String> {
    let mut db = lock_database(&database).map_err(|_| "WORKFLOW_DATABASE")?;
    task_workflow_store::list(&mut db, &date)
}
#[tauri::command]
pub fn write_task_workflow(
    database: State<'_, Database>,
    app: AppHandle,
    request: WorkflowWrite,
) -> Result<WorkflowSnapshot, String> {
    let result = {
        let mut db = lock_database(&database).map_err(|_| "WORKFLOW_DATABASE")?;
        let by = device_id(&db).map_err(|_| "WORKFLOW_DATABASE")?;
        task_workflow_store::write(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}
