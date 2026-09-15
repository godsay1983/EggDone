use crate::task_checklist_editor::{self, EditorRequest, EditorResult};
use crate::task_checklist_views::ChecklistEditorSnapshot;
use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_checklist_store::{self, ChecklistSave},
    task_checklist_views::{self, ChecklistPanelSnapshot, ChecklistProgress},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn resolve_checklist_rule_time(
    schedule: crate::recurrence::RecurrenceSchedule,
    timezone: Option<String>,
) -> Result<Option<i64>, String> {
    crate::recurrence_time::recurrence_due_at(
        &schedule.anchor_date,
        schedule.local_time_minutes,
        timezone.as_deref(),
    )
}

#[tauri::command]
pub fn read_task_checklist_editor(
    database: State<'_, Database>,
    uuid: String,
) -> Result<ChecklistEditorSnapshot, String> {
    let mut db = lock_database(&database)?;
    task_checklist_views::read_editor(&mut db, &uuid)
}

#[tauri::command]
pub fn create_task_checklist_editor(
    database: State<'_, Database>,
    app: AppHandle,
    request: EditorRequest,
) -> Result<EditorResult, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_checklist_editor::create(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn save_task_checklist_editor(
    database: State<'_, Database>,
    app: AppHandle,
    request: EditorRequest,
) -> Result<EditorResult, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_checklist_editor::save(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn read_task_checklist(
    database: State<'_, Database>,
    uuid: String,
) -> Result<ChecklistPanelSnapshot, String> {
    let mut db = lock_database(&database)?;
    task_checklist_views::read(&mut db, &uuid)
}
#[tauri::command]
pub fn list_task_checklist_progress(
    database: State<'_, Database>,
) -> Result<Vec<ChecklistProgress>, String> {
    let mut db = lock_database(&database)?;
    task_checklist_views::progress(&mut db)
}
#[tauri::command]
pub fn save_task_checklist(
    database: State<'_, Database>,
    app: AppHandle,
    request: ChecklistSave,
) -> Result<i64, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_checklist_store::save(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}
