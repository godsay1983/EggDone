use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{State, WebviewWindow};

use crate::db::Database;

#[derive(Debug, Serialize)]
pub struct Todo {
    id: i64,
    title: String,
    completed: bool,
    created_at: String,
    updated_at: String,
}

#[tauri::command]
pub fn list_todos(database: State<'_, Database>) -> Result<Vec<Todo>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())?;
    let mut statement = connection
        .prepare(
            "
            SELECT id, title, completed, created_at, updated_at
            FROM todos
            ORDER BY completed ASC, created_at DESC, id DESC
            ",
        )
        .map_err(database_error)?;

    let rows = statement
        .query_map([], |row| {
            Ok(Todo {
                id: row.get(0)?,
                title: row.get(1)?,
                completed: row.get::<_, i64>(2)? != 0,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

#[tauri::command]
pub fn create_todo(title: String, database: State<'_, Database>) -> Result<Todo, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务内容不能为空".to_string());
    }

    let connection = database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())?;
    connection
        .execute("INSERT INTO todos (title) VALUES (?1)", params![title])
        .map_err(database_error)?;

    find_todo(&connection, connection.last_insert_rowid())?
        .ok_or_else(|| "新建任务后未能读取记录".to_string())
}

#[tauri::command]
pub fn set_todo_completed(
    id: i64,
    completed: bool,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())?;
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET completed = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?2
            ",
            params![completed, id],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(&connection, id)?.ok_or_else(|| "更新后未能读取任务".to_string())
}

#[tauri::command]
pub fn delete_todo(id: i64, database: State<'_, Database>) -> Result<(), String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())?;
    connection
        .execute("DELETE FROM todos WHERE id = ?1", params![id])
        .map_err(database_error)?;
    Ok(())
}

#[tauri::command]
pub fn hide_panel(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

fn find_todo(connection: &rusqlite::Connection, id: i64) -> Result<Option<Todo>, String> {
    connection
        .query_row(
            "
            SELECT id, title, completed, created_at, updated_at
            FROM todos
            WHERE id = ?1
            ",
            params![id],
            |row| {
                Ok(Todo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    completed: row.get::<_, i64>(2)? != 0,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}
