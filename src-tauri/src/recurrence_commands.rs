use crate::{
    db::{device_id, now_millis, Database},
    recurrence_editor::{self, RuleEditRequest},
    recurrence_protocol::RecurrenceRule,
    recurrence_store,
};
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct EditorContext {
    rules: Vec<RecurrenceRule>,
    device_id: String,
}

#[tauri::command]
pub fn recurrence_editor_context(database: State<'_, Database>) -> Result<EditorContext, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "RECURRENCE_DATABASE")?;
    Ok(EditorContext {
        rules: recurrence_store::snapshot(&connection)?.document.rules,
        device_id: device_id(&connection).map_err(|_| "RECURRENCE_DATABASE")?,
    })
}

#[tauri::command]
pub fn save_recurrence_rule(
    app: AppHandle,
    database: State<'_, Database>,
    request: RuleEditRequest,
) -> Result<RecurrenceRule, String> {
    let result = {
        let mut connection = database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE")?;
        if request.rule.updated_by != device_id(&connection).map_err(|_| "RECURRENCE_DATABASE")? {
            return Err("INVALID_RECURRENCE_EDIT".into());
        }
        recurrence_editor::save_rule(&mut connection, &request)
    };
    if result.is_ok() {
        crate::tray::update_task_badge(&app);
    }
    result
}

#[tauri::command]
pub fn stop_recurrence_rule(
    app: AppHandle,
    database: State<'_, Database>,
    expected: RecurrenceRule,
) -> Result<RecurrenceRule, String> {
    let result = {
        let mut connection = database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE")?;
        let device = device_id(&connection).map_err(|_| "RECURRENCE_DATABASE")?;
        recurrence_editor::stop_rule(&mut connection, &expected, now_millis(), &device)
    };
    if result.is_ok() {
        crate::tray::update_task_badge(&app);
    }
    result
}
