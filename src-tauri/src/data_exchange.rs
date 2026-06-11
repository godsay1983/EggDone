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
const TODO_NOTE_MAX_CHARS: usize = 1000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct TransferTodo {
    uuid: String,
    title: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    group_uuid: Option<String>,
    completed: bool,
    #[serde(default)]
    pinned: bool,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    deleted_at: Option<i64>,
    #[serde(default)]
    archived_at: Option<i64>,
    #[serde(default)]
    due_date: Option<String>,
    #[serde(default)]
    due_at: Option<i64>,
    #[serde(default)]
    reminder_at: Option<i64>,
    #[serde(default)]
    repeat_rule: Option<String>,
    #[serde(default)]
    repeat_next_due_date: Option<String>,
    #[serde(default)]
    repeat_series_uuid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct TransferGroup {
    uuid: String,
    name: String,
    color: String,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TodoExport {
    format_version: u32,
    exported_at: i64,
    #[serde(default)]
    groups: Vec<TransferGroup>,
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
        groups: read_all_groups(&connection)?,
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
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<ImportResult, String> {
    let import = read_import_file(Path::new(&path))?;
    let result = {
        let mut connection = lock_database(&database)?;
        merge_transfer(&mut connection, &import.groups, &import.todos)
    };
    if result.is_ok() {
        crate::tray::update_task_badge(&app);
    }
    result
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

    let mut group_uuids = HashSet::new();
    for group in &import.groups {
        if Uuid::parse_str(&group.uuid).is_err() {
            return Err(format!("分组 UUID 无效：{}", group.uuid));
        }
        if !group_uuids.insert(&group.uuid) {
            return Err(format!("导入文件包含重复分组 UUID：{}", group.uuid));
        }
        if group.name.trim().is_empty() {
            return Err("导入文件包含空分组名称".to_string());
        }
        if group.created_at < 0
            || group.updated_at < 0
            || group.deleted_at.is_some_and(|value| value < 0)
        {
            return Err("导入文件包含无效分组时间戳".to_string());
        }
    }

    let mut uuids = HashSet::new();
    for todo in &import.todos {
        if Uuid::parse_str(&todo.uuid).is_err() {
            return Err(format!("任务 UUID 无效：{}", todo.uuid));
        }
        if todo
            .group_uuid
            .as_deref()
            .is_some_and(|value| Uuid::parse_str(value).is_err())
        {
            return Err(format!(
                "任务分组 UUID 无效：{}",
                todo.group_uuid.as_deref().unwrap_or_default()
            ));
        }
        if !uuids.insert(&todo.uuid) {
            return Err(format!("导入文件包含重复 UUID：{}", todo.uuid));
        }
        if todo
            .repeat_series_uuid
            .as_deref()
            .is_some_and(|value| Uuid::parse_str(value).is_err())
        {
            return Err(format!(
                "任务重复系列 UUID 无效：{}",
                todo.repeat_series_uuid.as_deref().unwrap_or_default()
            ));
        }
        if todo.title.trim().is_empty() {
            return Err("导入文件包含空标题任务".to_string());
        }
        if todo
            .note
            .as_deref()
            .is_some_and(|value| value.chars().count() > TODO_NOTE_MAX_CHARS)
        {
            return Err(format!(
                "导入文件包含超过 {TODO_NOTE_MAX_CHARS} 个字符的备注"
            ));
        }
        if todo.created_at < 0
            || todo.updated_at < 0
            || todo.completed_at.is_some_and(|value| value < 0)
            || todo.deleted_at.is_some_and(|value| value < 0)
            || todo.archived_at.is_some_and(|value| value < 0)
            || todo.due_at.is_some_and(|value| value < 0)
            || todo.reminder_at.is_some_and(|value| value < 0)
        {
            return Err("导入文件包含无效时间戳".to_string());
        }
        if todo
            .due_date
            .as_deref()
            .is_some_and(|value| !is_valid_date_only(value))
        {
            return Err("导入文件包含无效到期日期".to_string());
        }
        if todo.due_date.is_some() && todo.due_at.is_some() {
            return Err("导入文件包含重复到期信息".to_string());
        }
        if todo.repeat_rule.is_some() && todo.due_date.is_none() {
            return Err("导入文件包含缺少到期日期的重复任务".to_string());
        }
        validate_repeat_fields(
            todo.repeat_rule.as_deref(),
            todo.repeat_next_due_date.as_deref(),
            "导入文件",
        )?;
    }
    Ok(())
}

fn read_all_groups(connection: &Connection) -> Result<Vec<TransferGroup>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT uuid, name, color, sort_order, created_at, updated_at, deleted_at
            FROM groups
            ORDER BY sort_order ASC, created_at ASC, id ASC
            ",
        )
        .map_err(database_error)?;
    let groups = statement
        .query_map([], map_transfer_group)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(groups)
}

fn read_all_todos(connection: &Connection) -> Result<Vec<TransferTodo>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT uuid, title, group_uuid, completed, pinned, sort_order, created_at, updated_at,
                   completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                   repeat_rule, repeat_next_due_date, repeat_series_uuid, note
            FROM todos
            ORDER BY completed ASC, pinned DESC, sort_order ASC, created_at DESC, id DESC
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

fn merge_transfer(
    connection: &mut Connection,
    imported_groups: &[TransferGroup],
    imported: &[TransferTodo],
) -> Result<ImportResult, String> {
    let local_versions = local_versions(connection)?;
    let local_group_versions = local_group_versions(connection)?;
    let local_device_id = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let mut result = ImportResult {
        added: 0,
        updated: 0,
        unchanged: 0,
    };

    for group in imported_groups {
        match local_group_versions.get(&group.uuid) {
            None => insert_group(&transaction, group, &local_device_id)?,
            Some(local_updated_at) if group.updated_at > *local_updated_at => {
                update_group(&transaction, group, &local_device_id)?;
            }
            Some(_) => {}
        }
    }

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

fn local_group_versions(connection: &Connection) -> Result<HashMap<String, i64>, String> {
    let mut statement = connection
        .prepare("SELECT uuid, updated_at FROM groups")
        .map_err(database_error)?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(database_error)?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(database_error)
}

fn insert_group(
    connection: &Connection,
    group: &TransferGroup,
    updated_by: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO groups (
                uuid, name, color, sort_order, created_at, updated_at, deleted_at, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                group.uuid,
                group.name.trim(),
                group.color,
                group.sort_order,
                group.created_at,
                group.updated_at,
                group.deleted_at,
                updated_by,
            ],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn update_group(
    connection: &Connection,
    group: &TransferGroup,
    updated_by: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE groups
            SET name = ?1, color = ?2, sort_order = ?3, created_at = ?4,
                updated_at = ?5, deleted_at = ?6, updated_by = ?7
            WHERE uuid = ?8
            ",
            params![
                group.name.trim(),
                group.color,
                group.sort_order,
                group.created_at,
                group.updated_at,
                group.deleted_at,
                updated_by,
                group.uuid,
            ],
        )
        .map(|_| ())
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
                uuid, title, group_uuid, completed, pinned, sort_order, created_at, updated_at,
                completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, note, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ",
            params![
                todo.uuid,
                todo.title.trim(),
                todo.group_uuid,
                todo.completed,
                todo.pinned,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                todo.archived_at,
                todo.due_date,
                todo.due_at,
                todo.reminder_at,
                todo.repeat_rule,
                todo.repeat_next_due_date,
                todo.repeat_series_uuid,
                todo.note,
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
            SET title = ?1, completed = ?2, pinned = ?3, sort_order = ?4,
                created_at = ?5, updated_at = ?6, completed_at = ?7,
                deleted_at = ?8, archived_at = ?9, due_date = ?10, due_at = ?11, reminder_at = ?12,
                repeat_rule = ?13, repeat_next_due_date = ?14,
                repeat_series_uuid = ?15, note = ?16, group_uuid = ?17, updated_by = ?18
            WHERE uuid = ?19
            ",
            params![
                todo.title.trim(),
                todo.completed,
                todo.pinned,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                todo.archived_at,
                todo.due_date,
                todo.due_at,
                todo.reminder_at,
                todo.repeat_rule,
                todo.repeat_next_due_date,
                todo.repeat_series_uuid,
                todo.note,
                todo.group_uuid,
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
        group_uuid: row.get(2)?,
        completed: row.get::<_, i64>(3)? != 0,
        pinned: row.get::<_, i64>(4)? != 0,
        sort_order: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        completed_at: row.get(8)?,
        deleted_at: row.get(9)?,
        archived_at: row.get(10)?,
        due_date: row.get(11)?,
        due_at: row.get(12)?,
        reminder_at: row.get(13)?,
        repeat_rule: row.get(14)?,
        repeat_next_due_date: row.get(15)?,
        repeat_series_uuid: row.get(16)?,
        note: row.get(17)?,
    })
}

fn validate_repeat_fields(
    repeat_rule: Option<&str>,
    repeat_next_due_date: Option<&str>,
    source: &str,
) -> Result<(), String> {
    if let Some(rule) = repeat_rule {
        match rule {
            "daily" | "weekly" | "monthly" | "weekdays" => {}
            _ => return Err(format!("{source}包含无效重复规则")),
        }
        let Some(next_due_date) = repeat_next_due_date else {
            return Err(format!("{source}包含缺失的下次重复日期"));
        };
        if !is_valid_date_only(next_due_date) {
            return Err(format!("{source}包含无效下次重复日期"));
        }
    } else if repeat_next_due_date.is_some() {
        return Err(format!("{source}包含孤立的下次重复日期"));
    }
    Ok(())
}

fn map_transfer_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<TransferGroup> {
    Ok(TransferGroup {
        uuid: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
    })
}

fn is_valid_date_only(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }

    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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
            note: None,
            group_uuid: None,
            completed: false,
            pinned: false,
            sort_order: 0,
            created_at: 1,
            updated_at,
            completed_at: None,
            deleted_at: None,
            archived_at: None,
            due_date: None,
            due_at: None,
            reminder_at: None,
            repeat_rule: None,
            repeat_next_due_date: None,
            repeat_series_uuid: None,
        }
    }

    fn group(uuid: &str, name: &str, updated_at: i64) -> TransferGroup {
        TransferGroup {
            uuid: uuid.to_string(),
            name: name.to_string(),
            color: "yellow".to_string(),
            sort_order: 0,
            created_at: 1,
            updated_at,
            deleted_at: None,
        }
    }

    #[test]
    fn export_and_import_round_trip_including_deleted_todos() {
        let mut source = connection();
        let mut active = todo("00000000-0000-4000-8000-000000000001", "active", 2);
        active.pinned = true;
        active.note = Some("exported note".to_string());
        active.archived_at = Some(4);
        active.due_date = Some("2026-06-10".to_string());
        active.repeat_rule = Some("daily".to_string());
        active.repeat_next_due_date = Some("2026-06-11".to_string());
        let work = group("00000000-0000-4000-8000-0000000000aa", "工作", 2);
        active.group_uuid = Some(work.uuid.clone());
        let mut deleted = todo("00000000-0000-4000-8000-000000000002", "deleted", 3);
        deleted.deleted_at = Some(3);
        merge_transfer(
            &mut source,
            std::slice::from_ref(&work),
            &[active.clone(), deleted.clone()],
        )
        .unwrap();

        let export = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 10,
            groups: read_all_groups(&source).unwrap(),
            todos: read_all_todos(&source).unwrap(),
        };
        let json = serde_json::to_string(&export).unwrap();
        let exported: TodoExport = serde_json::from_str(&json).unwrap();
        validate_import(&exported).unwrap();
        let mut destination = connection();
        let result = merge_transfer(&mut destination, &exported.groups, &exported.todos).unwrap();

        assert_eq!(result.added, 2);
        assert_eq!(read_all_groups(&destination).unwrap(), vec![work]);
        assert_eq!(read_all_todos(&destination).unwrap(), vec![active, deleted]);
    }

    #[test]
    fn creates_a_readable_sqlite_backup() {
        let mut source = connection();
        let active = todo("00000000-0000-4000-8000-000000000005", "backup", 2);
        merge_transfer(&mut source, &[], &[active]).unwrap();
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
        merge_transfer(&mut connection, &[], &[todo(uuid, "local", 10)]).unwrap();

        let old_result = merge_transfer(&mut connection, &[], &[todo(uuid, "old", 9)]).unwrap();
        assert_eq!(old_result.unchanged, 1);

        let new_result = merge_transfer(&mut connection, &[], &[todo(uuid, "new", 11)]).unwrap();
        assert_eq!(new_result.updated, 1);
        assert_eq!(read_all_todos(&connection).unwrap()[0].title, "new");
    }

    #[test]
    fn rejects_future_versions_and_duplicate_uuids() {
        let shared = todo("00000000-0000-4000-8000-000000000004", "todo", 1);
        let future = TodoExport {
            format_version: FORMAT_VERSION + 1,
            exported_at: 1,
            groups: vec![],
            todos: vec![],
        };
        assert!(validate_import(&future).is_err());

        let duplicated = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 1,
            groups: vec![],
            todos: vec![shared.clone(), shared],
        };
        assert!(validate_import(&duplicated).is_err());
    }

    #[test]
    fn imports_legacy_json_without_pinned_field() {
        let json = r#"{
            "format_version": 1,
            "exported_at": 1,
            "todos": [{
                "uuid": "00000000-0000-4000-8000-000000000006",
                "title": "legacy",
                "completed": false,
                "sort_order": 0,
                "created_at": 1,
                "updated_at": 1,
                "completed_at": null,
                "deleted_at": null
            }]
        }"#;

        let import: TodoExport = serde_json::from_str(json).unwrap();
        assert!(import.groups.is_empty());
        assert!(!import.todos[0].pinned);
        assert_eq!(import.todos[0].note, None);
        assert_eq!(import.todos[0].archived_at, None);
        assert_eq!(import.todos[0].due_date, None);
        validate_import(&import).unwrap();
    }
}
