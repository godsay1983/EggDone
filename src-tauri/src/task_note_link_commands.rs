use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_note_link_operations::{self, LinkedTodoDraft},
    task_note_link_protocol::TaskNoteLink,
    task_note_link_views::{self, LinkScope, TaskNoteLinkView},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_task_note_links(
    database: State<'_, Database>,
    scope: LinkScope,
    uuid: String,
) -> Result<Vec<TaskNoteLinkView>, String> {
    let db = lock_database(&database)?;
    task_note_link_views::list(&db, scope, &uuid)
}

#[tauri::command]
pub fn get_task_note_link(
    database: State<'_, Database>,
    todo_uuid: String,
    note_uuid: String,
) -> Result<Option<TaskNoteLink>, String> {
    let db = lock_database(&database)?;
    task_note_link_views::pair(&db, &todo_uuid, &note_uuid)
}

#[tauri::command]
pub fn create_linked_todo(
    database: State<'_, Database>,
    app: AppHandle,
    draft: LinkedTodoDraft,
) -> Result<TaskNoteLink, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_note_link_operations::create(&mut db, &draft, now_millis(), &by)?
    };
    crate::tray::update_task_badge(&app);
    notify(&app);
    Ok(result)
}

#[tauri::command]
pub fn change_task_note_link(
    database: State<'_, Database>,
    app: AppHandle,
    todo_uuid: String,
    note_uuid: String,
    active: bool,
    expected: Option<TaskNoteLink>,
) -> Result<Option<TaskNoteLink>, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        task_note_link_operations::change(
            &mut db,
            &todo_uuid,
            &note_uuid,
            active,
            expected.as_ref(),
            now_millis(),
            &by,
        )?
    };
    notify(&app);
    Ok(result)
}

fn notify(app: &AppHandle) {
    let _ = app.emit_to("main", "todos-changed", ());
    let _ = app.emit_to("main", "notes-changed", ());
}
