use crate::archive_batch::{self, Job};
use crate::{
    archive::{self, Action, Cursor, Expected, Outcome, Page, Preview},
    commands::lock_database,
    db::{device_id, now_millis, Database},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn prepare_archive_batch(
    database: State<'_, Database>,
    operation: String,
    action: Action,
    targets: Vec<Expected>,
) -> Result<Job, String> {
    let mut c = lock_database(&database)?;
    archive_batch::prepare(&mut c, &operation, action, &targets)
}
#[tauri::command]
pub fn pending_archive_batches(database: State<'_, Database>) -> Result<Vec<Job>, String> {
    let mut c = lock_database(&database)?;
    archive_batch::pending(&mut c)
}
#[tauri::command]
pub fn dismiss_archive_batch(
    database: State<'_, Database>,
    operation: String,
) -> Result<(), String> {
    let mut c = lock_database(&database)?;
    archive_batch::dismiss(&mut c, &operation)
}
#[tauri::command]
pub fn run_archive_batch(
    database: State<'_, Database>,
    app: AppHandle,
    operation: String,
) -> Result<Job, String> {
    let result = {
        let mut c = lock_database(&database)?;
        let by = device_id(&c).map_err(|_| "ARCHIVE_DATABASE_FAILED")?;
        archive_batch::run(&mut c, &operation, 50, now_millis(), &by)?
    };
    crate::tray::update_task_badge(&app);
    let _ = app.emit_to("main", "todos-changed", ());
    let _ = app.emit_to("main", "notes-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn list_archived(
    database: State<'_, Database>,
    query: String,
    cursor: Option<Cursor>,
) -> Result<Page, String> {
    let mut connection = lock_database(&database)?;
    archive::list(&mut connection, &query, 50, cursor.as_ref())
}
#[tauri::command]
pub fn preview_archived(database: State<'_, Database>, uuid: String) -> Result<Preview, String> {
    let mut connection = lock_database(&database)?;
    archive::preview(&mut connection, &uuid)
}
#[tauri::command]
pub fn apply_archive_action(
    database: State<'_, Database>,
    app: AppHandle,
    operation: String,
    action: Action,
    expected: Expected,
) -> Result<Outcome, String> {
    let result = {
        let mut c = lock_database(&database)?;
        let by = device_id(&c).map_err(|_| "ARCHIVE_DATABASE_FAILED")?;
        archive::apply(&mut c, &operation, action, &expected, now_millis(), &by)?
    };
    crate::tray::update_task_badge(&app);
    let _ = app.emit_to("main", "todos-changed", ());
    let _ = app.emit_to("main", "notes-changed", ());
    Ok(result)
}
