use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const NOTE_ATTACHMENT_SYNC_FORMAT_VERSION: u32 = 1;
const DISPLAY_NAME_MAX_CHARS: usize = 255;
const MIME_TYPE_MAX_CHARS: usize = 100;
const MAX_PROTOCOL_BYTES: i64 = 8 * 1024 * 1024 * 1024;
const MAX_PREVIEW_BYTES: i64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncNoteAttachment {
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
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NoteAttachmentSyncDocument {
    pub format_version: u32,
    pub device_id: String,
    pub generated_at: i64,
    pub attachments: Vec<SyncNoteAttachment>,
}

pub(crate) fn validate_document(document: &NoteAttachmentSyncDocument) -> Result<(), String> {
    if document.format_version != NOTE_ATTACHMENT_SYNC_FORMAT_VERSION {
        return Err(format!(
            "附件同步文件版本 {} 不受支持",
            document.format_version
        ));
    }
    validate_uuid(&document.device_id, "附件同步文件 device_id 无效")?;
    if document.generated_at < 0 {
        return Err("附件同步文件 generated_at 无效".to_string());
    }

    let mut uuids = HashSet::new();
    for attachment in &document.attachments {
        validate_attachment(attachment)?;
        if !uuids.insert(&attachment.uuid) {
            return Err(format!("附件同步文件包含重复 UUID：{}", attachment.uuid));
        }
    }
    Ok(())
}

pub(crate) fn merge_documents(
    local: &NoteAttachmentSyncDocument,
    remote: &NoteAttachmentSyncDocument,
    generated_at: i64,
) -> Result<NoteAttachmentSyncDocument, String> {
    validate_document(local)?;
    validate_document(remote)?;
    if generated_at < 0 {
        return Err("附件同步时间无效".to_string());
    }

    let mut attachments = BTreeMap::<String, SyncNoteAttachment>::new();
    for attachment in local.attachments.iter().chain(&remote.attachments) {
        attachments
            .entry(attachment.uuid.clone())
            .and_modify(|current| {
                if compare_attachments(attachment, current).is_gt() {
                    *current = attachment.clone();
                }
            })
            .or_insert_with(|| attachment.clone());
    }

    Ok(NoteAttachmentSyncDocument {
        format_version: NOTE_ATTACHMENT_SYNC_FORMAT_VERSION,
        device_id: local.device_id.clone(),
        generated_at,
        attachments: attachments.into_values().collect(),
    })
}

pub(crate) fn compare_attachments(
    left: &SyncNoteAttachment,
    right: &SyncNoteAttachment,
) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.deleted_at.is_some().cmp(&right.deleted_at.is_some()))
        .then_with(|| left.updated_by.cmp(&right.updated_by))
        .then_with(|| left.note_uuid.cmp(&right.note_uuid))
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| left.display_name.cmp(&right.display_name))
        .then_with(|| left.mime_type.cmp(&right.mime_type))
        .then_with(|| left.byte_size.cmp(&right.byte_size))
        .then_with(|| left.sha256.cmp(&right.sha256))
        .then_with(|| left.preview_mime_type.cmp(&right.preview_mime_type))
        .then_with(|| left.preview_byte_size.cmp(&right.preview_byte_size))
        .then_with(|| left.preview_sha256.cmp(&right.preview_sha256))
        .then_with(|| left.width.cmp(&right.width))
        .then_with(|| left.height.cmp(&right.height))
        .then_with(|| left.sort_order.cmp(&right.sort_order))
        .then_with(|| left.created_at.cmp(&right.created_at))
}

fn validate_attachment(attachment: &SyncNoteAttachment) -> Result<(), String> {
    validate_uuid(&attachment.uuid, "同步附件 UUID 无效")?;
    validate_uuid(&attachment.note_uuid, "同步附件 note_uuid 无效")?;
    validate_uuid(&attachment.updated_by, "同步附件 updated_by 无效")?;
    if attachment.kind != "image" && attachment.kind != "file" {
        return Err("附件同步文件包含无效类型".to_string());
    }
    validate_display_name(&attachment.display_name)?;
    if !is_valid_mime_type(&attachment.mime_type) {
        return Err("附件同步文件包含无效 MIME 类型".to_string());
    }
    if attachment.byte_size <= 0 || attachment.byte_size > MAX_PROTOCOL_BYTES {
        return Err("附件同步文件包含无效文件大小".to_string());
    }
    validate_sha256(&attachment.sha256, "附件同步文件包含无效 SHA-256")?;
    if attachment.sort_order < 0
        || attachment.created_at < 0
        || attachment.updated_at < 0
        || attachment.deleted_at.is_some_and(|value| value < 0)
    {
        return Err("附件同步文件包含无效排序或时间戳".to_string());
    }

    if attachment.kind == "image" {
        if attachment.preview_mime_type.as_deref() != Some("image/jpeg") {
            return Err("图片附件缺少 JPEG 预览类型".to_string());
        }
        let preview_size = attachment
            .preview_byte_size
            .ok_or_else(|| "图片附件缺少预览大小".to_string())?;
        if preview_size <= 0 || preview_size > MAX_PREVIEW_BYTES {
            return Err("图片附件预览大小无效".to_string());
        }
        validate_sha256(
            attachment
                .preview_sha256
                .as_deref()
                .ok_or_else(|| "图片附件缺少预览 SHA-256".to_string())?,
            "图片附件预览 SHA-256 无效",
        )?;
        if attachment.width.is_none_or(|value| value <= 0)
            || attachment.height.is_none_or(|value| value <= 0)
        {
            return Err("图片附件尺寸无效".to_string());
        }
    } else if attachment.preview_mime_type.is_some()
        || attachment.preview_byte_size.is_some()
        || attachment.preview_sha256.is_some()
        || attachment.width.is_some()
        || attachment.height.is_some()
    {
        return Err("普通附件不能包含图片预览字段".to_string());
    }

    Ok(())
}

fn validate_uuid(value: &str, message: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| message.to_string())
}

fn validate_display_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.chars().count() > DISPLAY_NAME_MAX_CHARS
        || value
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err("附件同步文件包含无效显示名称".to_string());
    }
    Ok(())
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

fn validate_sha256(value: &str, message: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
    {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE_A: &str = "11111111-1111-4111-8111-111111111111";
    const DEVICE_B: &str = "22222222-2222-4222-8222-222222222222";

    #[test]
    fn merge_is_stable_and_keeps_newer_attachment() {
        let older = document(DEVICE_A, attachment("旧图片.jpg", 1000, DEVICE_A));
        let newer = document(DEVICE_B, attachment("新图片.jpg", 2000, DEVICE_B));

        let left = merge_documents(&older, &newer, 3000).unwrap();
        let right = merge_documents(&newer, &older, 3000).unwrap();

        assert_eq!(left.attachments[0].display_name, "新图片.jpg");
        assert_eq!(left.attachments, right.attachments);
    }

    #[test]
    fn deletion_wins_equal_timestamp_conflict() {
        let active = attachment("活动.jpg", 1000, DEVICE_B);
        let mut deleted = attachment("删除.jpg", 1000, DEVICE_A);
        deleted.deleted_at = Some(1000);

        let merged = merge_documents(
            &document(DEVICE_B, active),
            &document(DEVICE_A, deleted),
            3000,
        )
        .unwrap();

        assert_eq!(merged.attachments[0].deleted_at, Some(1000));
    }

    #[test]
    fn rejects_invalid_preview_and_duplicate_uuid() {
        let mut invalid = attachment("无效.jpg", 1000, DEVICE_A);
        invalid.preview_mime_type = Some("image/png".to_string());
        assert!(validate_document(&document(DEVICE_A, invalid)).is_err());

        let duplicate = attachment("重复.jpg", 1000, DEVICE_A);
        let mut duplicate_document = document(DEVICE_A, duplicate.clone());
        duplicate_document.attachments.push(duplicate);
        assert!(validate_document(&duplicate_document).is_err());
    }

    #[test]
    fn accepts_shared_cross_platform_fixture() {
        let fixture = include_str!("../../docs/fixtures/note-attachments-sync-v1.json");
        let document = serde_json::from_str::<NoteAttachmentSyncDocument>(fixture).unwrap();

        validate_document(&document).unwrap();
        assert_eq!(document.attachments.len(), 2);
        assert_eq!(document.attachments[0].mime_type, "image/heic");
        assert_eq!(document.attachments[1].deleted_at, Some(3000));
    }

    fn document(device_id: &str, value: SyncNoteAttachment) -> NoteAttachmentSyncDocument {
        NoteAttachmentSyncDocument {
            format_version: NOTE_ATTACHMENT_SYNC_FORMAT_VERSION,
            device_id: device_id.to_string(),
            generated_at: 2000,
            attachments: vec![value],
        }
    }

    fn attachment(display_name: &str, updated_at: i64, updated_by: &str) -> SyncNoteAttachment {
        SyncNoteAttachment {
            uuid: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_string(),
            note_uuid: "cccccccc-cccc-4ccc-8ccc-cccccccccccc".to_string(),
            kind: "image".to_string(),
            display_name: display_name.to_string(),
            mime_type: "image/jpeg".to_string(),
            byte_size: 4096,
            sha256: "a".repeat(64),
            preview_mime_type: Some("image/jpeg".to_string()),
            preview_byte_size: Some(1024),
            preview_sha256: Some("b".repeat(64)),
            width: Some(1920),
            height: Some(1080),
            sort_order: 0,
            created_at: 100,
            updated_at,
            deleted_at: None,
            updated_by: updated_by.to_string(),
        }
    }
}
