use crate::{
    commands::lock_database,
    db::Database,
    work_review::{self, ReviewCursor, ReviewPage, ReviewQuery, ReviewSnapshot},
};
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

async fn read<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Database>();
        let mut connection = lock_database(&state).map_err(|_| "REVIEW_DATABASE")?;
        action(&mut connection)
    })
    .await
    .map_err(|_| "REVIEW_DATABASE".to_string())?
}
#[tauri::command]
pub async fn list_work_review(
    app: AppHandle,
    query: ReviewQuery,
    cursor: Option<ReviewCursor>,
) -> Result<ReviewPage, String> {
    read(app, move |db| {
        work_review::list(db, &query, cursor.as_ref())
    })
    .await
}
#[tauri::command]
pub async fn snapshot_work_review(
    app: AppHandle,
    query: ReviewQuery,
) -> Result<ReviewSnapshot, String> {
    read(app, move |db| work_review::snapshot(db, &query)).await
}
#[tauri::command]
pub async fn validate_work_review(
    app: AppHandle,
    query: ReviewQuery,
    snapshot_token: String,
) -> Result<bool, String> {
    read(app, move |db| {
        work_review::validate(db, &query, &snapshot_token)
    })
    .await
}
