use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::db::{device_id, now_millis};

pub(crate) const IMAGE_UPLOAD_MAX_BYTES: i64 = 10 * 1024 * 1024;
const FILE_UPLOAD_MAX_BYTES: i64 = 20 * 1024 * 1024;
pub(crate) const PREVIEW_MAX_BYTES: i64 = 2 * 1024 * 1024;
const DISPLAY_NAME_MAX_CHARS: usize = 255;
const MIME_TYPE_MAX_CHARS: usize = 100;
const TRANSFER_STATES: [&str; 8] = [
    "pending_upload",
    "uploading",
    "uploaded",
    "synced",
    "remote_only",
    "downloading",
    "cached",
    "failed",
];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct NoteAttachment {
    pub id: i64,
    pub uuid: String,
    pub note_uuid: String,
    pub kind: String,
    pub display_name: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub preview_mime_type: Option<String>,
    pub preview_byte_size: Option<i64>,
    pub preview_sha256: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub updated_by: String,
    pub local_original_path: Option<String>,
    pub local_preview_path: Option<String>,
    pub transfer_state: String,
    pub transfer_error: Option<String>,
    pub remote_uploaded: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct NewNoteAttachment {
    pub uuid: String,
    pub note_uuid: String,
    pub kind: String,
    pub display_name: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub preview_mime_type: Option<String>,
    pub preview_byte_size: Option<i64>,
    pub preview_sha256: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub sort_order: i64,
    pub local_original_path: String,
    pub local_preview_path: Option<String>,
}

pub(crate) fn list_active_by_note(
    connection: &Connection,
    note_uuid: &str,
) -> Result<Vec<NoteAttachment>, String> {
    validate_uuid(note_uuid, "便签 UUID 无效")?;
    let mut statement = connection
        .prepare(&format!(
            "{}
             WHERE note_uuid = ?1 AND deleted_at IS NULL
             ORDER BY sort_order ASC, uuid ASC",
            select_columns()
        ))
        .map_err(database_error)?;
    let attachments = statement
        .query_map(params![note_uuid], map_attachment)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(attachments)
}

pub(crate) fn list_pending_transfers(
    connection: &Connection,
) -> Result<Vec<NoteAttachment>, String> {
    let mut statement = connection
        .prepare(&format!(
            "{}
             WHERE deleted_at IS NULL
               AND transfer_state IN ('pending_upload', 'uploading', 'uploaded', 'failed')
               AND NOT (transfer_state = 'failed' AND remote_uploaded = 1)
             ORDER BY updated_at ASC, uuid ASC",
            select_columns()
        ))
        .map_err(database_error)?;
    let attachments = statement
        .query_map([], map_attachment)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(attachments)
}

pub(crate) fn create_pending(
    connection: &Connection,
    input: &NewNoteAttachment,
) -> Result<NoteAttachment, String> {
    validate_new_attachment(input)?;
    require_active_note(connection, &input.note_uuid)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let now = now_millis();
    connection
        .execute(
            "
            INSERT INTO note_attachments (
                uuid, note_uuid, kind, display_name, mime_type, byte_size, sha256,
                preview_mime_type, preview_byte_size, preview_sha256, width, height,
                sort_order, created_at, updated_at, deleted_at, updated_by,
                local_original_path, local_preview_path, transfer_state,
                transfer_error, remote_uploaded
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?14, NULL, ?15,
                ?16, ?17, 'pending_upload', NULL, 0
            )
            ",
            params![
                input.uuid,
                input.note_uuid,
                input.kind,
                input.display_name,
                input.mime_type,
                input.byte_size,
                input.sha256,
                input.preview_mime_type,
                input.preview_byte_size,
                input.preview_sha256,
                input.width,
                input.height,
                input.sort_order,
                now,
                updated_by,
                input.local_original_path,
                input.local_preview_path,
            ],
        )
        .map_err(database_error)?;
    require_by_uuid(connection, &input.uuid)
}

pub(crate) fn reorder_active_by_note(
    connection: &mut Connection,
    note_uuid: &str,
    ordered_uuids: &[String],
) -> Result<Vec<NoteAttachment>, String> {
    validate_uuid(note_uuid, "便签 UUID 无效")?;
    let current = list_active_by_note(connection, note_uuid)?;
    if current.len() != ordered_uuids.len() {
        return Err("附件排序列表与当前便签不一致".to_string());
    }

    let current_uuids = current
        .iter()
        .map(|attachment| attachment.uuid.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::with_capacity(ordered_uuids.len());
    for uuid in ordered_uuids {
        validate_uuid(uuid, "附件 UUID 无效")?;
        if !current_uuids.contains(uuid.as_str()) || !seen.insert(uuid.as_str()) {
            return Err("附件排序列表与当前便签不一致".to_string());
        }
    }

    if ordered_uuids.is_empty() {
        return Ok(current);
    }

    let now = current.iter().fold(now_millis(), |latest, attachment| {
        latest.max(attachment.updated_at.saturating_add(1))
    });
    let updated_by = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for (index, uuid) in ordered_uuids.iter().enumerate() {
        let changed = transaction
            .execute(
                "
                UPDATE note_attachments
                SET sort_order = ?1, updated_at = ?2, updated_by = ?3
                WHERE uuid = ?4 AND note_uuid = ?5 AND deleted_at IS NULL
                ",
                params![index as i64 * 1_000, now, updated_by, uuid, note_uuid],
            )
            .map_err(database_error)?;
        require_changed(changed)?;
    }
    transaction.commit().map_err(database_error)?;

    list_active_by_note(connection, note_uuid)
}

pub(crate) fn set_transfer_state(
    connection: &Connection,
    uuid: &str,
    state: &str,
    error: Option<&str>,
    remote_uploaded: bool,
) -> Result<NoteAttachment, String> {
    validate_uuid(uuid, "附件 UUID 无效")?;
    if !TRANSFER_STATES.contains(&state) {
        return Err("附件传输状态无效".to_string());
    }
    if state == "synced" && !remote_uploaded {
        return Err("附件尚未上传，不能标记为已同步".to_string());
    }
    let changed = connection
        .execute(
            "
            UPDATE note_attachments
            SET transfer_state = ?1, transfer_error = ?2,
                remote_uploaded = ?3
            WHERE uuid = ?4
            ",
            params![state, error, remote_uploaded, uuid],
        )
        .map_err(database_error)?;
    if changed == 0 {
        return Err("附件不存在".to_string());
    }
    require_by_uuid(connection, uuid)
}

pub(crate) fn set_cached_file(
    connection: &Connection,
    uuid: &str,
    file_name: &str,
    relative_path: &str,
) -> Result<NoteAttachment, String> {
    validate_uuid(uuid, "附件 UUID 无效")?;
    validate_relative_path(relative_path)?;
    let expected_path = format!("note-assets/{uuid}/{file_name}");
    if relative_path != expected_path {
        return Err("附件缓存路径无效".to_string());
    }
    let changed = match file_name {
        "original" => connection.execute(
            "
            UPDATE note_attachments
            SET local_original_path = ?1, transfer_state = 'cached',
                transfer_error = NULL, remote_uploaded = 1
            WHERE uuid = ?2 AND deleted_at IS NULL
            ",
            params![relative_path, uuid],
        ),
        "preview.jpg" => connection.execute(
            "
            UPDATE note_attachments
            SET local_preview_path = ?1,
                transfer_state = CASE
                    WHEN local_original_path IS NULL THEN 'remote_only'
                    ELSE 'cached'
                END,
                transfer_error = NULL, remote_uploaded = 1
            WHERE uuid = ?2 AND deleted_at IS NULL
            ",
            params![relative_path, uuid],
        ),
        _ => return Err("附件文件名无效".to_string()),
    }
    .map_err(database_error)?;
    require_changed(changed)?;
    require_by_uuid(connection, uuid)
}

pub(crate) fn set_download_failed(
    connection: &Connection,
    uuid: &str,
    file_name: &str,
    error: &str,
) -> Result<NoteAttachment, String> {
    validate_uuid(uuid, "附件 UUID 无效")?;
    let changed = match file_name {
        "original" => connection.execute(
            "
            UPDATE note_attachments
            SET local_original_path = NULL, transfer_state = 'failed',
                transfer_error = ?1, remote_uploaded = 1
            WHERE uuid = ?2 AND deleted_at IS NULL
            ",
            params![error, uuid],
        ),
        "preview.jpg" => connection.execute(
            "
            UPDATE note_attachments
            SET local_preview_path = NULL, transfer_state = 'failed',
                transfer_error = ?1, remote_uploaded = 1
            WHERE uuid = ?2 AND deleted_at IS NULL
            ",
            params![error, uuid],
        ),
        _ => return Err("附件文件名无效".to_string()),
    }
    .map_err(database_error)?;
    require_changed(changed)?;
    require_by_uuid(connection, uuid)
}

pub(crate) fn soft_delete(connection: &Connection, uuid: &str) -> Result<NoteAttachment, String> {
    validate_uuid(uuid, "附件 UUID 无效")?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let now = now_millis();
    let changed = connection
        .execute(
            "
            UPDATE note_attachments
            SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE uuid = ?3 AND deleted_at IS NULL
            ",
            params![now, updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    require_by_uuid(connection, uuid)
}

pub(crate) fn restore(connection: &Connection, uuid: &str) -> Result<NoteAttachment, String> {
    validate_uuid(uuid, "附件 UUID 无效")?;
    let current = require_by_uuid(connection, uuid)?;
    require_active_note(connection, &current.note_uuid)?;
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE note_attachments
            SET deleted_at = NULL, updated_at = ?1, updated_by = ?2
            WHERE uuid = ?3 AND deleted_at IS NOT NULL
            ",
            params![now_millis(), updated_by, uuid],
        )
        .map_err(database_error)?;
    require_changed(changed)?;
    require_by_uuid(connection, uuid)
}

pub(crate) fn require_by_uuid(
    connection: &Connection,
    uuid: &str,
) -> Result<NoteAttachment, String> {
    connection
        .query_row(
            &format!("{} WHERE uuid = ?1", select_columns()),
            params![uuid],
            map_attachment,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "附件不存在".to_string())
}

fn select_columns() -> &'static str {
    "SELECT id, uuid, note_uuid, kind, display_name, mime_type, byte_size, sha256,
            preview_mime_type, preview_byte_size, preview_sha256, width, height,
            sort_order, created_at, updated_at, deleted_at, updated_by,
            local_original_path, local_preview_path, transfer_state,
            transfer_error, remote_uploaded
     FROM note_attachments"
}

fn map_attachment(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteAttachment> {
    Ok(NoteAttachment {
        id: row.get(0)?,
        uuid: row.get(1)?,
        note_uuid: row.get(2)?,
        kind: row.get(3)?,
        display_name: row.get(4)?,
        mime_type: row.get(5)?,
        byte_size: row.get(6)?,
        sha256: row.get(7)?,
        preview_mime_type: row.get(8)?,
        preview_byte_size: row.get(9)?,
        preview_sha256: row.get(10)?,
        width: row.get(11)?,
        height: row.get(12)?,
        sort_order: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        deleted_at: row.get(16)?,
        updated_by: row.get(17)?,
        local_original_path: row.get(18)?,
        local_preview_path: row.get(19)?,
        transfer_state: row.get(20)?,
        transfer_error: row.get(21)?,
        remote_uploaded: row.get::<_, i64>(22)? != 0,
    })
}

fn validate_new_attachment(input: &NewNoteAttachment) -> Result<(), String> {
    validate_uuid(&input.uuid, "附件 UUID 无效")?;
    validate_uuid(&input.note_uuid, "便签 UUID 无效")?;
    if input.kind != "image" && input.kind != "file" {
        return Err("附件类型无效".to_string());
    }
    if input.display_name.trim().is_empty()
        || input.display_name.chars().count() > DISPLAY_NAME_MAX_CHARS
        || input
            .display_name
            .chars()
            .any(|value| value.is_control() || value == '/' || value == '\\')
    {
        return Err("附件名称无效".to_string());
    }
    if !is_valid_mime_type(&input.mime_type) {
        return Err("附件 MIME 类型无效".to_string());
    }
    let max_bytes = if input.kind == "image" {
        IMAGE_UPLOAD_MAX_BYTES
    } else {
        FILE_UPLOAD_MAX_BYTES
    };
    if input.byte_size <= 0 || input.byte_size > max_bytes {
        return Err("附件大小超出限制".to_string());
    }
    validate_sha256(&input.sha256)?;
    if input.sort_order < 0 {
        return Err("附件排序无效".to_string());
    }
    validate_relative_path(&input.local_original_path)?;

    if input.kind == "image" {
        if input.preview_mime_type.as_deref() != Some("image/jpeg") {
            return Err("图片附件缺少 JPEG 预览".to_string());
        }
        if input
            .preview_byte_size
            .is_none_or(|value| value <= 0 || value > PREVIEW_MAX_BYTES)
        {
            return Err("图片预览大小无效".to_string());
        }
        validate_sha256(
            input
                .preview_sha256
                .as_deref()
                .ok_or_else(|| "图片预览摘要缺失".to_string())?,
        )?;
        if input.width.is_none_or(|value| value <= 0) || input.height.is_none_or(|value| value <= 0)
        {
            return Err("图片尺寸无效".to_string());
        }
        validate_relative_path(
            input
                .local_preview_path
                .as_deref()
                .ok_or_else(|| "图片预览路径缺失".to_string())?,
        )?;
    } else if input.preview_mime_type.is_some()
        || input.preview_byte_size.is_some()
        || input.preview_sha256.is_some()
        || input.width.is_some()
        || input.height.is_some()
        || input.local_preview_path.is_some()
    {
        return Err("普通附件不能包含图片预览字段".to_string());
    }
    Ok(())
}

fn require_active_note(connection: &Connection, uuid: &str) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM notes WHERE uuid = ?1 AND deleted_at IS NULL)",
            params![uuid],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if exists {
        Ok(())
    } else {
        Err("便签不存在或已删除".to_string())
    }
}

fn validate_uuid(value: &str, message: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| message.to_string())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
    {
        Ok(())
    } else {
        Err("附件 SHA-256 无效".to_string())
    }
}

fn is_valid_mime_type(value: &str) -> bool {
    if value.is_empty() || value.len() > MIME_TYPE_MAX_CHARS || !value.is_ascii() {
        return false;
    }
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && !subtype.contains('/')
        && kind.bytes().all(is_mime_token_byte)
        && subtype.bytes().all(is_mime_token_byte)
}

fn is_mime_token_byte(value: u8) -> bool {
    value.is_ascii_alphanumeric()
        || matches!(
            value,
            b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
        )
}

fn validate_relative_path(value: &str) -> Result<(), String> {
    let normalized = value.replace('\\', "/");
    let has_drive_prefix = normalized
        .as_bytes()
        .get(1)
        .is_some_and(|character| *character == b':');
    if normalized.is_empty()
        || normalized.starts_with('/')
        || has_drive_prefix
        || normalized.split('/').any(|segment| segment == "..")
    {
        Err("附件本地路径无效".to_string())
    } else {
        Ok(())
    }
}

fn require_changed(changed: usize) -> Result<(), String> {
    if changed == 0 {
        Err("附件不存在或已删除".to_string())
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
    use crate::{
        db::{configure_connection, migrate},
        notes,
    };

    const NOTE_ID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    const ATTACHMENT_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const SECOND_ATTACHMENT_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    #[test]
    fn creates_and_tracks_pending_attachment() {
        let connection = connection();
        insert_note(&connection);

        let created = create_pending(&connection, &image_input()).unwrap();
        assert_eq!(created.transfer_state, "pending_upload");
        assert!(!created.remote_uploaded);
        assert_eq!(list_pending_transfers(&connection).unwrap().len(), 1);

        let protocol_updated_at = created.updated_at;
        let synced = set_transfer_state(&connection, ATTACHMENT_ID, "synced", None, true).unwrap();
        assert!(synced.remote_uploaded);
        assert_eq!(synced.updated_at, protocol_updated_at);
        assert_eq!(list_pending_transfers(&connection).unwrap().len(), 0);
    }

    #[test]
    fn soft_deletes_restores_and_cascades_with_note() {
        let connection = connection();
        insert_note(&connection);
        create_pending(&connection, &image_input()).unwrap();

        let deleted = soft_delete(&connection, ATTACHMENT_ID).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(list_active_by_note(&connection, NOTE_ID)
            .unwrap()
            .is_empty());
        restore(&connection, ATTACHMENT_ID).unwrap();

        notes::soft_delete(&connection, NOTE_ID).unwrap();
        let note_deleted_at: Option<i64> = connection
            .query_row(
                "SELECT deleted_at FROM notes WHERE uuid = ?1",
                params![NOTE_ID],
                |row| row.get(0),
            )
            .unwrap();
        let cascade_deleted = require_by_uuid(&connection, ATTACHMENT_ID).unwrap();
        assert_eq!(cascade_deleted.deleted_at, note_deleted_at);

        notes::restore(&connection, NOTE_ID).unwrap();
        assert_eq!(
            require_by_uuid(&connection, ATTACHMENT_ID)
                .unwrap()
                .deleted_at,
            None
        );
    }

    #[test]
    fn reorders_complete_attachment_set_atomically() {
        let mut connection = connection();
        insert_note(&connection);
        create_pending(&connection, &image_input()).unwrap();
        let mut second = image_input();
        second.uuid = SECOND_ATTACHMENT_ID.to_string();
        second.sort_order = 1_000;
        second.local_original_path = format!("note-assets/{SECOND_ATTACHMENT_ID}/original");
        second.local_preview_path = Some(format!("note-assets/{SECOND_ATTACHMENT_ID}/preview.jpg"));
        create_pending(&connection, &second).unwrap();

        let reordered = reorder_active_by_note(
            &mut connection,
            NOTE_ID,
            &[SECOND_ATTACHMENT_ID.to_string(), ATTACHMENT_ID.to_string()],
        )
        .unwrap();
        assert_eq!(reordered[0].uuid, SECOND_ATTACHMENT_ID);
        assert_eq!(reordered[0].sort_order, 0);
        assert_eq!(reordered[1].uuid, ATTACHMENT_ID);
        assert_eq!(reordered[1].sort_order, 1_000);

        let error = reorder_active_by_note(
            &mut connection,
            NOTE_ID,
            &[SECOND_ATTACHMENT_ID.to_string()],
        )
        .unwrap_err();
        assert_eq!(error, "附件排序列表与当前便签不一致");
    }

    #[test]
    fn tracks_download_cache_without_requeueing_remote_failures() {
        let connection = connection();
        insert_note(&connection);
        create_pending(&connection, &image_input()).unwrap();
        set_transfer_state(&connection, ATTACHMENT_ID, "synced", None, true).unwrap();
        connection
            .execute(
                "UPDATE note_attachments
                 SET local_original_path = NULL, local_preview_path = NULL,
                     transfer_state = 'remote_only'
                 WHERE uuid = ?1",
                params![ATTACHMENT_ID],
            )
            .unwrap();

        let preview_path = format!("note-assets/{ATTACHMENT_ID}/preview.jpg");
        let preview =
            set_cached_file(&connection, ATTACHMENT_ID, "preview.jpg", &preview_path).unwrap();
        assert_eq!(preview.transfer_state, "remote_only");
        assert_eq!(
            preview.local_preview_path.as_deref(),
            Some(preview_path.as_str())
        );

        let failed =
            set_download_failed(&connection, ATTACHMENT_ID, "original", "网络不可用").unwrap();
        assert_eq!(failed.transfer_state, "failed");
        assert!(failed.remote_uploaded);
        assert!(list_pending_transfers(&connection).unwrap().is_empty());

        let original_path = format!("note-assets/{ATTACHMENT_ID}/original");
        let cached =
            set_cached_file(&connection, ATTACHMENT_ID, "original", &original_path).unwrap();
        assert_eq!(cached.transfer_state, "cached");
        assert_eq!(
            cached.local_original_path.as_deref(),
            Some(original_path.as_str())
        );
    }

    #[test]
    fn rejects_invalid_paths_and_preview_metadata() {
        let connection = connection();
        insert_note(&connection);
        let mut invalid_path = image_input();
        invalid_path.local_original_path = "C:/temp/image.jpg".to_string();
        assert!(create_pending(&connection, &invalid_path).is_err());

        let mut missing_preview = image_input();
        missing_preview.preview_sha256 = None;
        assert!(create_pending(&connection, &missing_preview).is_err());
    }

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    fn insert_note(connection: &Connection) {
        connection
            .execute(
                "
                INSERT INTO notes (
                    uuid, title, content, color, pinned,
                    created_at, updated_at, deleted_at, updated_by
                ) VALUES (?1, '附件便签', '正文', 'default', 0, 100, 100, NULL, ?2)
                ",
                params![NOTE_ID, device_id(connection).unwrap()],
            )
            .unwrap();
    }

    fn image_input() -> NewNoteAttachment {
        NewNoteAttachment {
            uuid: ATTACHMENT_ID.to_string(),
            note_uuid: NOTE_ID.to_string(),
            kind: "image".to_string(),
            display_name: "会议白板.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            byte_size: 4096,
            sha256: "a".repeat(64),
            preview_mime_type: Some("image/jpeg".to_string()),
            preview_byte_size: Some(1024),
            preview_sha256: Some("b".repeat(64)),
            width: Some(1920),
            height: Some(1080),
            sort_order: 0,
            local_original_path: format!("note-assets/{ATTACHMENT_ID}/original"),
            local_preview_path: Some(format!("note-assets/{ATTACHMENT_ID}/preview.jpg")),
        }
    }
}
