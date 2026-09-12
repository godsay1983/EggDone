use crate::{
    commands::lock_database,
    content_search::{self, SearchPage, SearchScope, SearchTarget},
    db::Database,
};
use tauri::State;

#[tauri::command]
pub fn search_content(
    database: State<'_, Database>,
    scope: SearchScope,
    query: String,
    offset: u32,
    limit: u32,
) -> Result<SearchPage, String> {
    let connection = lock_database(&database)?;
    content_search::search(&connection, scope, &query, offset, limit)
}

#[tauri::command]
pub fn resolve_search_target(
    database: State<'_, Database>,
    scope: SearchScope,
    uuid: String,
) -> Result<SearchTarget, String> {
    let connection = lock_database(&database)?;
    content_search::resolve(&connection, scope, &uuid)
}
