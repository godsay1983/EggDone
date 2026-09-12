use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    note_history::{self, HistoryPreview, HistorySummary},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_note_history(
    database: State<'_, Database>,
    uuid: String,
) -> Result<Vec<HistorySummary>, String> {
    let connection = lock_database(&database)?;
    note_history::list(&connection, &uuid)
}

#[tauri::command]
pub fn preview_note_history(
    database: State<'_, Database>,
    uuid: String,
    id: i64,
) -> Result<HistoryPreview, String> {
    let connection = lock_database(&database)?;
    note_history::preview(&connection, &uuid, id)
}

#[tauri::command]
pub fn restore_note_history(
    database: State<'_, Database>,
    app: AppHandle,
    expected: HistoryPreview,
) -> Result<bool, String> {
    let changed = {
        let mut connection = lock_database(&database)?;
        let by = device_id(&connection).map_err(|_| "NOTE_HISTORY_DATABASE_FAILED".to_string())?;
        note_history::restore(&mut connection, &expected, now_millis(), &by)?
    };
    if changed {
        let _ = app.emit_to("main", "notes-changed", ());
    }
    Ok(changed)
}
