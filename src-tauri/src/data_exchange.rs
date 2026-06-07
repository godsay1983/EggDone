use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{backup::Backup, params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use uuid::Uuid;

use crate::{
    db::{device_id, now_millis, Database},
    tray::PanelState,
};

const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct TransferTodo {
    uuid: String,
    title: String,
    completed: bool,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    deleted_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TodoExport {
    format_version: u32,
    exported_at: i64,
    todos: Vec<TransferTodo>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImportPreview {
    path: String,
    file_name: String,
    total: usize,
    added: usize,
    updated: usize,
    unchanged: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImportResult {
    added: usize,
    updated: usize,
    unchanged: usize,
}

#[tauri::command]
pub fn export_todos(
    app: AppHandle,
    database: State<'_, Database>,
    panel_state: State<'_, PanelState>,
) -> Result<Option<String>, String> {
    let Some(path) = pick_save_path(
        &app,
        &panel_state,
        "eggdone-todos.json",
        "EggDone JSON",
        &["json"],
    )?
    else {
        return Ok(None);
    };

    let connection = lock_database(&database)?;
    let export = TodoExport {
        format_version: FORMAT_VERSION,
        exported_at: now_millis(),
        todos: read_all_todos(&connection)?,
    };
    let json = serde_json::to_string_pretty(&export)
        .map_err(|error| format!("生成导出文件失败：{error}"))?;
    fs::write(&path, json).map_err(|error| format!("写入导出文件失败：{error}"))?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn preview_todo_import(
    app: AppHandle,
    database: State<'_, Database>,
    panel_state: State<'_, PanelState>,
) -> Result<Option<ImportPreview>, String> {
    let Some(path) = pick_open_path(&app, &panel_state, "EggDone JSON", &["json"])? else {
        return Ok(None);
    };
    let import = read_import_file(&path)?;
    let connection = lock_database(&database)?;
    let preview = build_preview(&connection, &path, &import.todos)?;
    Ok(Some(preview))
}

#[tauri::command]
pub fn confirm_todo_import(
    path: String,
    database: State<'_, Database>,
) -> Result<ImportResult, String> {
    let import = read_import_file(Path::new(&path))?;
    let mut connection = lock_database(&database)?;
    merge_todos(&mut connection, &import.todos)
}

#[tauri::command]
pub fn backup_database(
    app: AppHandle,
    database: State<'_, Database>,
    panel_state: State<'_, PanelState>,
) -> Result<Option<String>, String> {
    let Some(path) = pick_save_path(
        &app,
        &panel_state,
        "eggdone-backup.sqlite3",
        "SQLite Database",
        &["sqlite3", "db"],
    )?
    else {
        return Ok(None);
    };

    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("无法覆盖备份文件：{error}"))?;
    }

    let source = lock_database(&database)?;
    backup_connection(&source, &path)?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

fn pick_save_path(
    app: &AppHandle,
    panel_state: &PanelState,
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Result<Option<PathBuf>, String> {
    panel_state.begin_dialog();
    let selected = app
        .dialog()
        .file()
        .set_file_name(file_name)
        .add_filter(filter_name, extensions)
        .blocking_save_file();
    panel_state.end_dialog();
    selected.map(file_path_to_path).transpose()
}

fn pick_open_path(
    app: &AppHandle,
    panel_state: &PanelState,
    filter_name: &str,
    extensions: &[&str],
) -> Result<Option<PathBuf>, String> {
    panel_state.begin_dialog();
    let selected = app
        .dialog()
        .file()
        .add_filter(filter_name, extensions)
        .blocking_pick_file();
    panel_state.end_dialog();
    selected.map(file_path_to_path).transpose()
}

fn file_path_to_path(path: FilePath) -> Result<PathBuf, String> {
    path.into_path()
        .map_err(|error| format!("选择的文件不是本地路径：{error}"))
}

fn backup_connection(source: &Connection, path: &Path) -> Result<(), String> {
    let mut destination =
        Connection::open(path).map_err(|error| format!("创建备份数据库失败：{error}"))?;
    let backup = Backup::new(source, &mut destination)
        .map_err(|error| format!("初始化数据库备份失败：{error}"))?;
    backup
        .run_to_completion(32, std::time::Duration::from_millis(10), None)
        .map_err(|error| format!("数据库备份失败：{error}"))
}

fn read_import_file(path: &Path) -> Result<TodoExport, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("读取导入文件失败：{error}"))?;
    let import: TodoExport =
        serde_json::from_str(&contents).map_err(|error| format!("JSON 格式无效：{error}"))?;
    validate_import(&import)?;
    Ok(import)
}

fn validate_import(import: &TodoExport) -> Result<(), String> {
    if import.format_version > FORMAT_VERSION {
        return Err(format!(
            "导入文件版本 {} 高于当前支持的版本 {}",
            import.format_version, FORMAT_VERSION
        ));
    }
    if import.format_version == 0 {
        return Err("导入文件缺少有效的 format_version".to_string());
    }

    let mut uuids = HashSet::new();
    for todo in &import.todos {
        if Uuid::parse_str(&todo.uuid).is_err() {
            return Err(format!("任务 UUID 无效：{}", todo.uuid));
        }
        if !uuids.insert(&todo.uuid) {
            return Err(format!("导入文件包含重复 UUID：{}", todo.uuid));
        }
        if todo.title.trim().is_empty() {
            return Err("导入文件包含空标题任务".to_string());
        }
        if todo.created_at < 0
            || todo.updated_at < 0
            || todo.completed_at.is_some_and(|value| value < 0)
            || todo.deleted_at.is_some_and(|value| value < 0)
        {
            return Err("导入文件包含无效时间戳".to_string());
        }
    }
    Ok(())
}

fn read_all_todos(connection: &Connection) -> Result<Vec<TransferTodo>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT uuid, title, completed, sort_order, created_at, updated_at,
                   completed_at, deleted_at
            FROM todos
            ORDER BY completed ASC, sort_order ASC, created_at DESC, id DESC
            ",
        )
        .map_err(database_error)?;
    let todos = statement
        .query_map([], map_transfer_todo)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(todos)
}

fn build_preview(
    connection: &Connection,
    path: &Path,
    imported: &[TransferTodo],
) -> Result<ImportPreview, String> {
    let local_versions = local_versions(connection)?;
    let mut added = 0;
    let mut updated = 0;
    let mut unchanged = 0;

    for todo in imported {
        match local_versions.get(&todo.uuid) {
            None => added += 1,
            Some(local_updated_at) if todo.updated_at > *local_updated_at => updated += 1,
            Some(_) => unchanged += 1,
        }
    }

    Ok(ImportPreview {
        path: path.to_string_lossy().into_owned(),
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("EggDone JSON")
            .to_string(),
        total: imported.len(),
        added,
        updated,
        unchanged,
    })
}

fn merge_todos(
    connection: &mut Connection,
    imported: &[TransferTodo],
) -> Result<ImportResult, String> {
    let local_versions = local_versions(connection)?;
    let local_device_id = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let mut result = ImportResult {
        added: 0,
        updated: 0,
        unchanged: 0,
    };

    for todo in imported {
        match local_versions.get(&todo.uuid) {
            None => {
                insert_todo(&transaction, todo, &local_device_id)?;
                result.added += 1;
            }
            Some(local_updated_at) if todo.updated_at > *local_updated_at => {
                update_todo(&transaction, todo, &local_device_id)?;
                result.updated += 1;
            }
            Some(_) => result.unchanged += 1,
        }
    }

    transaction.commit().map_err(database_error)?;
    Ok(result)
}

fn local_versions(connection: &Connection) -> Result<HashMap<String, i64>, String> {
    let mut statement = connection
        .prepare("SELECT uuid, updated_at FROM todos")
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(database_error)?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(database_error)
}

fn insert_todo(
    connection: &Connection,
    todo: &TransferTodo,
    updated_by: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO todos (
                uuid, title, completed, sort_order, created_at, updated_at,
                completed_at, deleted_at, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                todo.uuid,
                todo.title.trim(),
                todo.completed,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                updated_by,
            ],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn update_todo(
    connection: &Connection,
    todo: &TransferTodo,
    updated_by: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE todos
            SET title = ?1, completed = ?2, sort_order = ?3, created_at = ?4,
                updated_at = ?5, completed_at = ?6, deleted_at = ?7,
                updated_by = ?8
            WHERE uuid = ?9
            ",
            params![
                todo.title.trim(),
                todo.completed,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                updated_by,
                todo.uuid,
            ],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn map_transfer_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<TransferTodo> {
    Ok(TransferTodo {
        uuid: row.get(0)?,
        title: row.get(1)?,
        completed: row.get::<_, i64>(2)? != 0,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        completed_at: row.get(6)?,
        deleted_at: row.get(7)?,
    })
}

fn lock_database<'a>(
    database: &'a State<'_, Database>,
) -> Result<std::sync::MutexGuard<'a, Connection>, String> {
    database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    fn todo(uuid: &str, title: &str, updated_at: i64) -> TransferTodo {
        TransferTodo {
            uuid: uuid.to_string(),
            title: title.to_string(),
            completed: false,
            sort_order: 0,
            created_at: 1,
            updated_at,
            completed_at: None,
            deleted_at: None,
        }
    }

    #[test]
    fn export_and_import_round_trip_including_deleted_todos() {
        let mut source = connection();
        let active = todo("00000000-0000-4000-8000-000000000001", "active", 2);
        let mut deleted = todo("00000000-0000-4000-8000-000000000002", "deleted", 3);
        deleted.deleted_at = Some(3);
        merge_todos(&mut source, &[active.clone(), deleted.clone()]).unwrap();

        let export = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 10,
            todos: read_all_todos(&source).unwrap(),
        };
        let json = serde_json::to_string(&export).unwrap();
        let exported: TodoExport = serde_json::from_str(&json).unwrap();
        validate_import(&exported).unwrap();
        let mut destination = connection();
        let result = merge_todos(&mut destination, &exported.todos).unwrap();

        assert_eq!(result.added, 2);
        assert_eq!(read_all_todos(&destination).unwrap(), vec![active, deleted]);
    }

    #[test]
    fn creates_a_readable_sqlite_backup() {
        let mut source = connection();
        let active = todo("00000000-0000-4000-8000-000000000005", "backup", 2);
        merge_todos(&mut source, &[active]).unwrap();
        let path = std::env::temp_dir().join(format!("eggdone-{}.sqlite3", Uuid::new_v4()));

        backup_connection(&source, &path).unwrap();
        let destination = Connection::open(&path).unwrap();
        let count: i64 = destination
            .query_row("SELECT COUNT(*) FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        drop(destination);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn merge_updates_only_when_import_is_newer() {
        let mut connection = connection();
        let uuid = "00000000-0000-4000-8000-000000000003";
        merge_todos(&mut connection, &[todo(uuid, "local", 10)]).unwrap();

        let old_result = merge_todos(&mut connection, &[todo(uuid, "old", 9)]).unwrap();
        assert_eq!(old_result.unchanged, 1);

        let new_result = merge_todos(&mut connection, &[todo(uuid, "new", 11)]).unwrap();
        assert_eq!(new_result.updated, 1);
        assert_eq!(read_all_todos(&connection).unwrap()[0].title, "new");
    }

    #[test]
    fn rejects_future_versions_and_duplicate_uuids() {
        let shared = todo("00000000-0000-4000-8000-000000000004", "todo", 1);
        let future = TodoExport {
            format_version: FORMAT_VERSION + 1,
            exported_at: 1,
            todos: vec![],
        };
        assert!(validate_import(&future).is_err());

        let duplicated = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 1,
            todos: vec![shared.clone(), shared],
        };
        assert!(validate_import(&duplicated).is_err());
    }
}
