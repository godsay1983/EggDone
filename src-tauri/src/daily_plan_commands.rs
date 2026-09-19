use crate::{
    commands::lock_database,
    daily_plan_store::{self, DailyPlanSnapshot, DailyPlanWrite},
    db::{device_id, now_millis, Database},
};
use tauri::{AppHandle, Emitter, State};
#[tauri::command]
pub fn list_daily_plans(
    database: State<'_, Database>,
    date: String,
) -> Result<DailyPlanSnapshot, String> {
    let mut db = lock_database(&database)?;
    daily_plan_store::list(&mut db, &date)
}
#[tauri::command]
pub fn write_daily_plan(
    database: State<'_, Database>,
    app: AppHandle,
    request: DailyPlanWrite,
) -> Result<DailyPlanSnapshot, String> {
    let result = {
        let mut db = lock_database(&database)?;
        let by = device_id(&db).map_err(|e| e.to_string())?;
        daily_plan_store::write(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    Ok(result)
}
