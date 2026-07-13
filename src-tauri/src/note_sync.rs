use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::device_id;

pub(crate) const NOTE_SYNC_FORMAT_VERSION: u32 = 1;
const NOTE_TITLE_MAX_CHARS: usize = 100;
const NOTE_CONTENT_MAX_CHARS: usize = 20_000;
const NOTE_COLORS: [&str; 5] = ["default", "yellow", "pink", "green", "blue"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncNote {
    pub uuid: String,
    pub title: String,
    pub content: String,
    pub color: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub updated_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NoteSyncDocument {
    pub format_version: u32,
    pub device_id: String,
    pub generated_at: i64,
    pub notes: Vec<SyncNote>,
}

pub(crate) fn build_document(
    connection: &Connection,
    generated_at: i64,
) -> Result<NoteSyncDocument, String> {
    let mut statement = connection
        .prepare(
            "SELECT uuid, title, content, color, pinned, created_at, updated_at,
                    deleted_at, updated_by
             FROM notes
             ORDER BY uuid ASC",
        )
        .map_err(database_error)?;
    let notes = statement
        .query_map([], map_sync_note)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    Ok(NoteSyncDocument {
        format_version: NOTE_SYNC_FORMAT_VERSION,
        device_id: device_id(connection).map_err(database_error)?,
        generated_at,
        notes,
    })
}

pub(crate) fn merge_documents(
    local: &NoteSyncDocument,
    remote: &NoteSyncDocument,
    generated_at: i64,
) -> Result<NoteSyncDocument, String> {
    validate_document(local)?;
    validate_document(remote)?;

    let mut notes = BTreeMap::<String, SyncNote>::new();
    for note in local.notes.iter().chain(&remote.notes) {
        notes
            .entry(note.uuid.clone())
            .and_modify(|current| {
                if compare_notes(note, current).is_gt() {
                    *current = note.clone();
                }
            })
            .or_insert_with(|| note.clone());
    }

    Ok(NoteSyncDocument {
        format_version: NOTE_SYNC_FORMAT_VERSION,
        device_id: local.device_id.clone(),
        generated_at,
        notes: notes.into_values().collect(),
    })
}

pub(crate) fn merge_remote_document(
    connection: &mut Connection,
    remote: &NoteSyncDocument,
    generated_at: i64,
) -> Result<NoteSyncDocument, String> {
    let local = build_document(connection, generated_at)?;
    let merged = merge_documents(&local, remote, generated_at)?;
    let transaction = connection.transaction().map_err(database_error)?;

    for note in &merged.notes {
        transaction
            .execute(
                "INSERT INTO notes (
                    uuid, title, content, color, pinned, created_at, updated_at,
                    deleted_at, updated_by
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(uuid) DO UPDATE SET
                    title = excluded.title,
                    content = excluded.content,
                    color = excluded.color,
                    pinned = excluded.pinned,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at,
                    updated_by = excluded.updated_by",
                params![
                    note.uuid,
                    note.title.trim(),
                    note.content,
                    note.color,
                    note.pinned,
                    note.created_at,
                    note.updated_at,
                    note.deleted_at,
                    note.updated_by,
                ],
            )
            .map_err(database_error)?;
    }

    transaction.commit().map_err(database_error)?;
    Ok(merged)
}

pub(crate) fn validate_document(document: &NoteSyncDocument) -> Result<(), String> {
    if document.format_version != NOTE_SYNC_FORMAT_VERSION {
        return Err(format!(
            "便签同步文件版本 {} 不受支持",
            document.format_version
        ));
    }
    if Uuid::parse_str(&document.device_id).is_err() {
        return Err("便签同步文件 device_id 无效".to_string());
    }
    if document.generated_at < 0 {
        return Err("便签同步文件 generated_at 无效".to_string());
    }

    let mut uuids = HashSet::new();
    for note in &document.notes {
        if Uuid::parse_str(&note.uuid).is_err() {
            return Err(format!("同步便签 UUID 无效：{}", note.uuid));
        }
        if Uuid::parse_str(&note.updated_by).is_err() {
            return Err(format!("同步便签 updated_by 无效：{}", note.updated_by));
        }
        if !uuids.insert(&note.uuid) {
            return Err(format!("便签同步文件包含重复 UUID：{}", note.uuid));
        }
        if note.title.trim().is_empty() && note.content.trim().is_empty() {
            return Err("便签同步文件包含空白便签".to_string());
        }
        if note.title.chars().count() > NOTE_TITLE_MAX_CHARS {
            return Err(format!(
                "便签同步文件包含超过 {NOTE_TITLE_MAX_CHARS} 个字符的标题"
            ));
        }
        if note.content.chars().count() > NOTE_CONTENT_MAX_CHARS {
            return Err(format!(
                "便签同步文件包含超过 {NOTE_CONTENT_MAX_CHARS} 个字符的正文"
            ));
        }
        if !NOTE_COLORS.contains(&note.color.as_str()) {
            return Err("便签同步文件包含无效颜色".to_string());
        }
        if note.created_at < 0
            || note.updated_at < 0
            || note.deleted_at.is_some_and(|value| value < 0)
        {
            return Err("便签同步文件包含无效时间戳".to_string());
        }
    }
    Ok(())
}

fn compare_notes(left: &SyncNote, right: &SyncNote) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.deleted_at.is_some().cmp(&right.deleted_at.is_some()))
        .then_with(|| left.updated_by.cmp(&right.updated_by))
        .then_with(|| left.pinned.cmp(&right.pinned))
        .then_with(|| left.color.cmp(&right.color))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.content.cmp(&right.content))
        .then_with(|| left.created_at.cmp(&right.created_at))
}

fn map_sync_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncNote> {
    Ok(SyncNote {
        uuid: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        color: row.get(3)?,
        pinned: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        deleted_at: row.get(7)?,
        updated_by: row.get(8)?,
    })
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    const DEVICE_A: &str = "00000000-0000-4000-8000-00000000000a";
    const DEVICE_B: &str = "00000000-0000-4000-8000-00000000000b";
    const NOTE_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn document(device_id: &str, note: SyncNote) -> NoteSyncDocument {
        NoteSyncDocument {
            format_version: NOTE_SYNC_FORMAT_VERSION,
            device_id: device_id.to_string(),
            generated_at: 20,
            notes: vec![note],
        }
    }

    fn note(title: &str, updated_at: i64, updated_by: &str) -> SyncNote {
        SyncNote {
            uuid: NOTE_ID.to_string(),
            title: title.to_string(),
            content: "正文".to_string(),
            color: "yellow".to_string(),
            pinned: false,
            created_at: 1,
            updated_at,
            deleted_at: None,
            updated_by: updated_by.to_string(),
        }
    }

    #[test]
    fn newer_note_wins_in_both_merge_directions() {
        let older = document(DEVICE_A, note("旧", 10, DEVICE_A));
        let newer = document(DEVICE_B, note("新", 11, DEVICE_B));

        let left = merge_documents(&older, &newer, 30).unwrap();
        let right = merge_documents(&newer, &older, 30).unwrap();

        assert_eq!(left.notes[0].title, "新");
        assert_eq!(left.notes, right.notes);
    }

    #[test]
    fn deletion_wins_equal_timestamp_conflict() {
        let active = document(DEVICE_B, note("保留", 10, DEVICE_B));
        let mut deleted = note("删除", 10, DEVICE_A);
        deleted.deleted_at = Some(10);

        let merged = merge_documents(&active, &document(DEVICE_A, deleted), 30).unwrap();

        assert_eq!(merged.notes[0].deleted_at, Some(10));
    }

    #[test]
    fn applies_remote_note_and_preserves_tombstone() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        let mut remote_note = note("远端", 10, DEVICE_B);
        remote_note.deleted_at = Some(11);

        let merged =
            merge_remote_document(&mut connection, &document(DEVICE_B, remote_note), 20).unwrap();
        let exported = build_document(&connection, 21).unwrap();

        assert_eq!(merged.notes, exported.notes);
        assert_eq!(exported.notes[0].deleted_at, Some(11));
    }

    #[test]
    fn rejects_invalid_color_and_duplicate_uuid() {
        let mut invalid = note("无效", 10, DEVICE_A);
        invalid.color = "orange".to_string();
        assert!(validate_document(&document(DEVICE_A, invalid)).is_err());

        let duplicate = note("重复", 10, DEVICE_A);
        let mut duplicate_document = document(DEVICE_A, duplicate.clone());
        duplicate_document.notes.push(duplicate);
        assert!(validate_document(&duplicate_document).is_err());
    }

    #[test]
    fn accepts_shared_cross_platform_fixture() {
        let fixture = include_str!("../../docs/fixtures/notes-sync-v1.json");
        let document = serde_json::from_str::<NoteSyncDocument>(fixture).unwrap();

        validate_document(&document).unwrap();
        assert_eq!(document.notes[0].title, "购物清单");
        assert!(document.notes[0].pinned);
        assert_eq!(document.notes[1].deleted_at, Some(2500));
    }
}
