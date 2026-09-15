use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_template_protocol::{TaskTemplate, TemplatesDocument},
    task_template_store::{self, TemplateWrite},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_task_templates(database: State<'_, Database>) -> Result<TemplatesDocument, String> {
    let mut db = lock_database(&database)?;
    Ok(task_template_store::snapshot(&mut db)?.document)
}

#[tauri::command]
pub fn save_task_template(
    database: State<'_, Database>,
    app: AppHandle,
    request: TemplateWrite,
) -> Result<TaskTemplate, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_template_store::save(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}
