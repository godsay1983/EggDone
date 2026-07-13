use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::db::{device_id, now_millis};

const NOTE_TITLE_MAX_CHARS: usize = 100;
const NOTE_CONTENT_MAX_CHARS: usize = 20_000;
const DEFAULT_NOTE_COLOR: &str = "default";
const NOTE_COLORS: [&str; 5] = ["default", "yellow", "pink", "green", "blue"];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Note {
    id: i64,
    uuid: String,
    title: String,
    content: String,
    color: String,
    pinned: bool,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
    updated_by: String,
}

pub(crate) fn list_active(connection: &Connection) -> Result<Vec<Note>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, uuid, title, content, color, pinned,
                   created_at, updated_at, deleted_at, updated_by
            FROM notes
            WHERE deleted_at IS NULL
            ORDER BY pinned DESC, updated_at DESC, uuid ASC
            ",
        )
        .map_err(database_error)?;
    let notes = statement
        .query_map([], map_note)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(notes)
}

pub(crate) fn create(
    connection: &Connection,
    title: &str,
    content: &str,
    color: &str,
) -> Result<Note, String> {
    let (title, content) = normalize_text(title, content)?;
    let color = normalize_color(color)?;
    let uuid = Uuid::new_v4().to_string();
    let updated_by = device_id(connection).map_err(database_error)?;
    let now = now_millis();
    connection
        .execute(
            "
            INSERT INTO notes (
                uuid, title, content, color, pinned,
                created_at, updated_at, deleted_at, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, NULL, ?6)
            ",
            params![uuid, title, content, color, now, updated_by],
        )
        .map_err(database_error)?;
    find_by_uuid(connection, &uuid)
}

pub(crate) fn update(
    connection: &Connection,
    uuid: &str,
    title: &str,
    content: &str,
) -> Result<Note, String> {
    validate_uuid(uuid)?;
    let (title, content) = normalize_text(title, content)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE notes
            SET title = ?1, content = ?2, updated_at = ?3, updated_by = ?4
            WHERE uuid = ?5 AND deleted_at IS NULL
            ",
            params![title, content, now_millis(), updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    find_by_uuid(connection, uuid)
}

pub(crate) fn set_pinned(
    connection: &Connection,
    uuid: &str,
    pinned: bool,
) -> Result<Note, String> {
    validate_uuid(uuid)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE notes
            SET pinned = ?1, updated_at = ?2, updated_by = ?3
            WHERE uuid = ?4 AND deleted_at IS NULL
            ",
            params![pinned, now_millis(), updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    find_by_uuid(connection, uuid)
}

pub(crate) fn set_color(connection: &Connection, uuid: &str, color: &str) -> Result<Note, String> {
    validate_uuid(uuid)?;
    let color = normalize_color(color)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE notes
            SET color = ?1, updated_at = ?2, updated_by = ?3
            WHERE uuid = ?4 AND deleted_at IS NULL
            ",
            params![color, now_millis(), updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    find_by_uuid(connection, uuid)
}

pub(crate) fn soft_delete(connection: &Connection, uuid: &str) -> Result<Note, String> {
    validate_uuid(uuid)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let now = now_millis();
    let changed = connection
        .execute(
            "
            UPDATE notes
            SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE uuid = ?3 AND deleted_at IS NULL
            ",
            params![now, updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    find_by_uuid(connection, uuid)
}

pub(crate) fn restore(connection: &Connection, uuid: &str) -> Result<Note, String> {
    validate_uuid(uuid)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE notes
            SET deleted_at = NULL, updated_at = ?1, updated_by = ?2
            WHERE uuid = ?3 AND deleted_at IS NOT NULL
            ",
            params![now_millis(), updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    find_by_uuid(connection, uuid)
}

fn find_by_uuid(connection: &Connection, uuid: &str) -> Result<Note, String> {
    connection
        .query_row(
            "
            SELECT id, uuid, title, content, color, pinned,
                   created_at, updated_at, deleted_at, updated_by
            FROM notes
            WHERE uuid = ?1
            ",
            params![uuid],
            map_note,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "便签不存在".to_string())
}

fn map_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        uuid: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        color: row.get(4)?,
        pinned: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
        updated_by: row.get(9)?,
    })
}

fn normalize_text(title: &str, content: &str) -> Result<(String, String), String> {
    let title = title.trim().to_string();
    let content = content.to_string();
    if title.trim().is_empty() && content.trim().is_empty() {
        return Err("便签标题和正文不能同时为空".to_string());
    }
    if title.chars().count() > NOTE_TITLE_MAX_CHARS {
        return Err(format!("便签标题不能超过 {NOTE_TITLE_MAX_CHARS} 个字符"));
    }
    if content.chars().count() > NOTE_CONTENT_MAX_CHARS {
        return Err(format!("便签正文不能超过 {NOTE_CONTENT_MAX_CHARS} 个字符"));
    }
    Ok((title, content))
}

fn normalize_color(color: &str) -> Result<&str, String> {
    let color = if color.trim().is_empty() {
        DEFAULT_NOTE_COLOR
    } else {
        color
    };
    NOTE_COLORS
        .contains(&color)
        .then_some(color)
        .ok_or_else(|| "便签颜色无效".to_string())
}

fn validate_uuid(uuid: &str) -> Result<(), String> {
    Uuid::parse_str(uuid)
        .map(|_| ())
        .map_err(|_| "便签 UUID 无效".to_string())
}

fn require_changed(changed: usize) -> Result<(), String> {
    if changed == 0 {
        Err("便签不存在或已删除".to_string())
    } else {
        Ok(())
    }
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

    #[test]
    fn creates_updates_and_lists_notes() {
        let connection = connection();
        let first = create(&connection, "第一条", "正文", "yellow").unwrap();
        let second = create(&connection, "第二条", "更多内容", "default").unwrap();

        set_pinned(&connection, &first.uuid, true).unwrap();
        set_color(&connection, &second.uuid, "blue").unwrap();
        let updated = update(&connection, &second.uuid, "第二条更新", "新正文").unwrap();

        let notes = list_active(&connection).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].uuid, first.uuid);
        assert!(notes[0].pinned);
        assert_eq!(updated.title, "第二条更新");
        assert_eq!(updated.content, "新正文");
        assert_eq!(updated.color, "blue");
    }

    #[test]
    fn soft_deletes_and_restores_note() {
        let connection = connection();
        let note = create(&connection, "待删除", "正文", "pink").unwrap();

        let deleted = soft_delete(&connection, &note.uuid).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(list_active(&connection).unwrap().is_empty());

        let restored = restore(&connection, &note.uuid).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(list_active(&connection).unwrap().len(), 1);
    }

    #[test]
    fn validates_content_and_color() {
        let connection = connection();
        assert!(create(&connection, "", "   ", "default").is_err());
        assert!(create(&connection, "标题", "正文", "orange").is_err());
        assert!(create(&connection, &"a".repeat(101), "正文", "default").is_err());
        assert!(create(&connection, "emoji", "😀", "green").is_ok());
    }
}
