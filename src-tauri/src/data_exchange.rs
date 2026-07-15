use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use rusqlite::{backup::Backup, params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    db::{device_id, now_millis, Database},
    note_asset_store::NoteAssetStore,
    note_attachment_sync::{self, NoteAttachmentSyncDocument, SyncNoteAttachment},
    note_sync::{self, NoteSyncDocument, SyncNote},
    schedule::{local_date_from_timestamp, timestamp_for_local_date},
    tray::PanelState,
};

const FORMAT_VERSION: u32 = 1;
const BACKUP_FORMAT_VERSION: u32 = 1;
const BACKUP_MAX_ENTRY_COUNT: usize = 10_000;
const BACKUP_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
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
    #[serde(default)]
    priority: i64,
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
    #[serde(default)]
    notes: Vec<SyncNote>,
    #[serde(default)]
    note_attachments: Vec<SyncNoteAttachment>,
    #[serde(default)]
    attachment_files_included: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupManifest {
    format_version: u32,
    created_at: i64,
    data: BackupManifestEntry,
    assets: Vec<BackupManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct BackupManifestEntry {
    path: String,
    byte_size: u64,
    sha256: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FullBackupExportResult {
    path: String,
    attachment_count: usize,
    file_count: usize,
    total_bytes: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImportPreview {
    path: String,
    file_name: String,
    total: usize,
    added: usize,
    updated: usize,
    unchanged: usize,
    note_total: usize,
    note_added: usize,
    note_updated: usize,
    note_unchanged: usize,
    attachment_total: usize,
    attachment_added: usize,
    attachment_updated: usize,
    attachment_unchanged: usize,
    attachment_files_included: bool,
    backup_file_count: usize,
    backup_total_bytes: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImportResult {
    added: usize,
    updated: usize,
    unchanged: usize,
    note_added: usize,
    note_updated: usize,
    note_unchanged: usize,
    attachment_added: usize,
    attachment_updated: usize,
    attachment_unchanged: usize,
    restored_file_count: usize,
}

struct ValidatedFullBackup {
    import: TodoExport,
    manifest: BackupManifest,
    total_bytes: u64,
}

struct InstalledAsset {
    uuid: String,
    had_previous: bool,
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
        "eggdone-data.json",
        "EggDone JSON",
        &["json"],
    )?
    else {
        return Ok(None);
    };

    let connection = lock_database(&database)?;
    let exported_at = now_millis();
    let export = TodoExport {
        format_version: FORMAT_VERSION,
        exported_at,
        groups: read_all_groups(&connection)?,
        todos: read_all_todos(&connection)?,
        notes: note_sync::build_document(&connection, exported_at)?.notes,
        note_attachments: note_attachment_sync::build_document(&connection, exported_at)?
            .attachments,
        attachment_files_included: false,
    };
    let json = serde_json::to_string_pretty(&export)
        .map_err(|error| format!("生成导出文件失败：{error}"))?;
    fs::write(&path, json).map_err(|error| format!("写入导出文件失败：{error}"))?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn export_full_backup(
    app: AppHandle,
    database: State<'_, Database>,
    panel_state: State<'_, PanelState>,
) -> Result<Option<FullBackupExportResult>, String> {
    let Some(path) = pick_save_path(
        &app,
        &panel_state,
        "eggdone-data.eggdone-backup",
        "EggDone Complete Backup",
        &["eggdone-backup"],
    )?
    else {
        return Ok(None);
    };

    let exported_at = now_millis();
    let connection = lock_database(&database)?;
    let attachment_document =
        note_attachment_sync::build_backup_document(&connection, exported_at)?;
    let export = TodoExport {
        format_version: FORMAT_VERSION,
        exported_at,
        groups: read_all_groups(&connection)?,
        todos: read_all_todos(&connection)?,
        notes: note_sync::build_document(&connection, exported_at)?.notes,
        note_attachments: attachment_document.attachments.clone(),
        attachment_files_included: true,
    };
    drop(connection);

    let data_bytes = serde_json::to_vec_pretty(&export)
        .map_err(|error| format!("生成完整备份数据失败：{error}"))?;
    let asset_store = NoteAssetStore::from_app(&app)?;
    let mut assets = Vec::<(BackupManifestEntry, PathBuf)>::new();
    let mut total_bytes = data_bytes.len() as u64;
    for attachment in attachment_document
        .attachments
        .iter()
        .filter(|attachment| attachment.deleted_at.is_none())
    {
        let original = asset_store
            .verified_asset_path(
                &attachment.uuid,
                "original",
                attachment.byte_size,
                &attachment.sha256,
            )
            .map_err(|error| {
                format!(
                    "附件“{}”尚未完整下载或校验失败，无法创建完整备份：{error}",
                    attachment.display_name
                )
            })?;
        push_backup_asset(
            &mut assets,
            &mut total_bytes,
            format!("note-assets/{}/original", attachment.uuid),
            original,
            attachment.byte_size as u64,
            &attachment.sha256,
        )?;

        if attachment.kind == "image" {
            let preview_size = attachment
                .preview_byte_size
                .ok_or_else(|| "图片附件缺少预览大小".to_string())?;
            let preview_sha256 = attachment
                .preview_sha256
                .as_deref()
                .ok_or_else(|| "图片附件缺少预览摘要".to_string())?;
            let preview = asset_store
                .verified_asset_path(
                    &attachment.uuid,
                    "preview.jpg",
                    preview_size,
                    preview_sha256,
                )
                .map_err(|error| {
                    format!(
                        "图片“{}”的预览尚未完整下载或校验失败，无法创建完整备份：{error}",
                        attachment.display_name
                    )
                })?;
            push_backup_asset(
                &mut assets,
                &mut total_bytes,
                format!("note-assets/{}/preview.jpg", attachment.uuid),
                preview,
                preview_size as u64,
                preview_sha256,
            )?;
        }
    }
    if assets.len() + 2 > BACKUP_MAX_ENTRY_COUNT {
        return Err("完整备份文件数量超过 10000 个限制".to_string());
    }

    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        created_at: exported_at,
        data: backup_manifest_entry("data.json".to_string(), &data_bytes),
        assets: assets.iter().map(|(entry, _)| entry.clone()).collect(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("生成完整备份清单失败：{error}"))?;
    total_bytes = total_bytes.saturating_add(manifest_bytes.len() as u64);
    if total_bytes > BACKUP_MAX_TOTAL_BYTES {
        return Err("完整备份解压后不能超过 512 MiB".to_string());
    }

    let temporary_path = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("eggdone-backup"),
        Uuid::new_v4()
    ));
    let write_result =
        write_full_backup_archive(&temporary_path, &manifest_bytes, &data_bytes, &assets);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("无法覆盖完整备份：{error}"))?;
    }
    fs::rename(&temporary_path, &path).map_err(|error| format!("保存完整备份失败：{error}"))?;

    Ok(Some(FullBackupExportResult {
        path: path.to_string_lossy().into_owned(),
        attachment_count: attachment_document
            .attachments
            .iter()
            .filter(|attachment| attachment.deleted_at.is_none())
            .count(),
        file_count: assets.len(),
        total_bytes,
    }))
}

fn push_backup_asset(
    assets: &mut Vec<(BackupManifestEntry, PathBuf)>,
    total_bytes: &mut u64,
    path: String,
    source_path: PathBuf,
    byte_size: u64,
    sha256: &str,
) -> Result<(), String> {
    *total_bytes = total_bytes.saturating_add(byte_size);
    if *total_bytes > BACKUP_MAX_TOTAL_BYTES {
        return Err("完整备份解压后不能超过 512 MiB".to_string());
    }
    assets.push((
        BackupManifestEntry {
            path,
            byte_size,
            sha256: sha256.to_string(),
        },
        source_path,
    ));
    Ok(())
}

fn backup_manifest_entry(path: String, bytes: &[u8]) -> BackupManifestEntry {
    BackupManifestEntry {
        path,
        byte_size: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}

fn write_full_backup_archive(
    path: &Path,
    manifest: &[u8],
    data: &[u8],
    assets: &[(BackupManifestEntry, PathBuf)],
) -> Result<(), String> {
    let file = File::create(path).map_err(|error| format!("创建完整备份失败：{error}"))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file("manifest.json", options)
        .map_err(|error| format!("写入完整备份清单失败：{error}"))?;
    archive
        .write_all(manifest)
        .map_err(|error| format!("写入完整备份清单失败：{error}"))?;
    archive
        .start_file("data.json", options)
        .map_err(|error| format!("写入完整备份数据失败：{error}"))?;
    archive
        .write_all(data)
        .map_err(|error| format!("写入完整备份数据失败：{error}"))?;
    for (entry, source_path) in assets {
        archive
            .start_file(&entry.path, options)
            .map_err(|error| format!("写入完整备份附件失败：{error}"))?;
        let mut source =
            File::open(source_path).map_err(|error| format!("打开完整备份附件失败：{error}"))?;
        io::copy(&mut source, &mut archive)
            .map_err(|error| format!("写入完整备份附件失败：{error}"))?;
    }
    archive
        .finish()
        .map_err(|error| format!("完成完整备份失败：{error}"))?;
    Ok(())
}

fn read_full_backup_archive(
    path: &Path,
    extraction_root: Option<&Path>,
) -> Result<ValidatedFullBackup, String> {
    let file = File::open(path).map_err(|error| format!("读取完整备份失败：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("完整备份 ZIP 无效：{error}"))?;
    if archive.len() == 0 || archive.len() > BACKUP_MAX_ENTRY_COUNT {
        return Err("完整备份条目数量无效或超过 10000 个限制".to_string());
    }

    let mut names = HashSet::new();
    let mut file_names = HashSet::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("读取完整备份条目失败：{error}"))?;
        let name = entry.name().to_string();
        validate_backup_entry_path(&name, entry.is_dir())?;
        if !names.insert(name.clone()) {
            return Err(format!("完整备份包含重复条目：{name}"));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("完整备份不允许符号链接：{name}"));
        }
        if !entry.is_dir() {
            file_names.insert(name);
            total_bytes = total_bytes.saturating_add(entry.size());
            if total_bytes > BACKUP_MAX_TOTAL_BYTES {
                return Err("完整备份解压后不能超过 512 MiB".to_string());
            }
        }
    }

    let manifest_bytes = read_archive_entry(&mut archive, "manifest.json", 4 * 1024 * 1024, None)?;
    let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("完整备份清单格式无效：{error}"))?;
    validate_backup_manifest(&manifest)?;

    let expected_files = std::iter::once("manifest.json".to_string())
        .chain(std::iter::once("data.json".to_string()))
        .chain(manifest.assets.iter().map(|entry| entry.path.clone()))
        .collect::<HashSet<_>>();
    if file_names != expected_files {
        let unexpected = file_names.difference(&expected_files).next();
        let missing = expected_files.difference(&file_names).next();
        return Err(match (unexpected, missing) {
            (Some(name), _) => format!("完整备份包含清单外条目：{name}"),
            (_, Some(name)) => format!("完整备份缺少条目：{name}"),
            _ => "完整备份条目与清单不一致".to_string(),
        });
    }

    let data_bytes = read_archive_entry(&mut archive, "data.json", manifest.data.byte_size, None)?;
    verify_manifest_bytes(&manifest.data, &data_bytes)?;
    let import: TodoExport = serde_json::from_slice(&data_bytes)
        .map_err(|error| format!("完整备份数据格式无效：{error}"))?;
    validate_complete_import(&import)?;
    validate_manifest_assets(&manifest, &import)?;

    if let Some(root) = extraction_root {
        if root.exists() {
            fs::remove_dir_all(root).map_err(|error| format!("清理恢复临时目录失败：{error}"))?;
        }
        fs::create_dir_all(root).map_err(|error| format!("创建恢复临时目录失败：{error}"))?;
    }
    for entry in &manifest.assets {
        let output = extraction_root.map(|root| {
            root.join(
                entry
                    .path
                    .strip_prefix("note-assets/")
                    .unwrap_or(&entry.path),
            )
        });
        let bytes = read_archive_entry(
            &mut archive,
            &entry.path,
            entry.byte_size,
            output.as_deref(),
        )?;
        if let Some(output) = output {
            let (size, sha256) = hash_backup_file(&output)?;
            if size as u64 != entry.byte_size || sha256 != entry.sha256 {
                return Err(format!("完整备份条目摘要不匹配：{}", entry.path));
            }
        } else {
            verify_manifest_bytes(entry, &bytes)?;
        }
    }

    Ok(ValidatedFullBackup {
        import,
        manifest,
        total_bytes,
    })
}

fn read_archive_entry(
    archive: &mut ZipArchive<File>,
    name: &str,
    max_bytes: u64,
    output_path: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("完整备份缺少条目：{name}"))?;
    if entry.size() > max_bytes || entry.size() > BACKUP_MAX_TOTAL_BYTES {
        return Err(format!("完整备份条目大小无效：{name}"));
    }
    let mut output = if let Some(path) = output_path {
        let parent = path
            .parent()
            .ok_or_else(|| "恢复附件路径无效".to_string())?;
        fs::create_dir_all(parent).map_err(|error| format!("创建恢复附件目录失败：{error}"))?;
        Some(File::create(path).map_err(|error| format!("创建恢复附件失败：{error}"))?)
    } else {
        None
    };
    let capacity = usize::try_from(entry.size().min(4 * 1024 * 1024)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let mut buffer = [0_u8; 64 * 1024];
    let mut read_bytes = 0_u64;
    loop {
        let count = entry
            .read(&mut buffer)
            .map_err(|error| format!("读取完整备份条目失败：{error}"))?;
        if count == 0 {
            break;
        }
        read_bytes = read_bytes.saturating_add(count as u64);
        if read_bytes > max_bytes {
            return Err(format!("完整备份条目超过清单大小：{name}"));
        }
        if let Some(file) = output.as_mut() {
            file.write_all(&buffer[..count])
                .map_err(|error| format!("写入恢复附件失败：{error}"))?;
        } else {
            bytes.extend_from_slice(&buffer[..count]);
        }
    }
    if read_bytes != entry.size() {
        return Err(format!("完整备份条目读取不完整：{name}"));
    }
    Ok(bytes)
}

fn validate_backup_entry_path(name: &str, is_directory: bool) -> Result<(), String> {
    if name.is_empty() || name.contains('\\') || name.starts_with('/') || name.contains(':') {
        return Err(format!("完整备份包含不安全路径：{name}"));
    }
    let trimmed = if is_directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if trimmed
        .split('/')
        .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(format!("完整备份包含不安全路径：{name}"));
    }
    if is_directory {
        let parts = trimmed.split('/').collect::<Vec<_>>();
        if parts.first() != Some(&"note-assets") || parts.len() > 2 {
            return Err(format!("完整备份包含无效目录：{name}"));
        }
        if parts.len() == 2 && Uuid::parse_str(parts[1]).is_err() {
            return Err(format!("完整备份包含无效附件目录：{name}"));
        }
    }
    Ok(())
}

fn validate_backup_manifest(manifest: &BackupManifest) -> Result<(), String> {
    if manifest.format_version != BACKUP_FORMAT_VERSION || manifest.created_at < 0 {
        return Err("完整备份清单版本或创建时间无效".to_string());
    }
    if manifest.data.path != "data.json"
        || manifest.data.byte_size == 0
        || !is_sha256_value(&manifest.data.sha256)
    {
        return Err("完整备份 data.json 清单无效".to_string());
    }
    let mut previous = None::<&str>;
    let mut paths = HashSet::new();
    for entry in &manifest.assets {
        validate_backup_entry_path(&entry.path, false)?;
        if !entry.path.starts_with("note-assets/")
            || entry.byte_size == 0
            || !is_sha256_value(&entry.sha256)
        {
            return Err(format!("完整备份附件清单无效：{}", entry.path));
        }
        if previous.is_some_and(|value| value >= entry.path.as_str()) {
            return Err("完整备份附件清单必须按路径升序排列".to_string());
        }
        if !paths.insert(entry.path.clone()) {
            return Err(format!("完整备份清单包含重复路径：{}", entry.path));
        }
        previous = Some(&entry.path);
    }
    Ok(())
}

fn validate_manifest_assets(manifest: &BackupManifest, import: &TodoExport) -> Result<(), String> {
    let entries = manifest
        .assets
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<HashMap<_, _>>();
    let mut expected = HashSet::new();
    for attachment in import
        .note_attachments
        .iter()
        .filter(|attachment| attachment.deleted_at.is_none())
    {
        let original = format!("note-assets/{}/original", attachment.uuid);
        let original_entry = entries
            .get(original.as_str())
            .ok_or_else(|| format!("完整备份缺少附件原文件：{}", attachment.display_name))?;
        if original_entry.byte_size != attachment.byte_size as u64
            || original_entry.sha256 != attachment.sha256
        {
            return Err(format!(
                "完整备份附件原文件清单不匹配：{}",
                attachment.display_name
            ));
        }
        expected.insert(original);
        if attachment.kind == "image" {
            let preview = format!("note-assets/{}/preview.jpg", attachment.uuid);
            let preview_entry = entries
                .get(preview.as_str())
                .ok_or_else(|| format!("完整备份缺少图片预览：{}", attachment.display_name))?;
            if preview_entry.byte_size != attachment.preview_byte_size.unwrap_or_default() as u64
                || preview_entry.sha256 != attachment.preview_sha256.as_deref().unwrap_or_default()
            {
                return Err(format!(
                    "完整备份图片预览清单不匹配：{}",
                    attachment.display_name
                ));
            }
            expected.insert(preview);
        }
    }
    if entries.len() != expected.len() {
        return Err("完整备份包含未被活动附件引用的二进制文件".to_string());
    }
    Ok(())
}

fn verify_manifest_bytes(entry: &BackupManifestEntry, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != entry.byte_size
        || format!("{:x}", Sha256::digest(bytes)) != entry.sha256
    {
        return Err(format!("完整备份条目摘要不匹配：{}", entry.path));
    }
    Ok(())
}

fn hash_backup_file(path: &Path) -> Result<(u64, String), String> {
    let mut file = File::open(path).map_err(|error| format!("打开恢复附件失败：{error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("读取恢复附件失败：{error}"))?;
        if count == 0 {
            break;
        }
        size = size.saturating_add(count as u64);
        if size > BACKUP_MAX_TOTAL_BYTES {
            return Err("恢复附件超过完整备份容量限制".to_string());
        }
        digest.update(&buffer[..count]);
    }
    Ok((size, format!("{:x}", digest.finalize())))
}

fn is_sha256_value(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
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
    let preview = build_preview(&connection, &path, &import)?;
    Ok(Some(preview))
}

#[tauri::command]
pub fn preview_full_backup_import(
    app: AppHandle,
    database: State<'_, Database>,
    panel_state: State<'_, PanelState>,
) -> Result<Option<ImportPreview>, String> {
    let Some(path) = pick_open_path(
        &app,
        &panel_state,
        "EggDone Complete Backup",
        &["eggdone-backup"],
    )?
    else {
        return Ok(None);
    };
    let validated = read_full_backup_archive(&path, None)?;
    let connection = lock_database(&database)?;
    let mut preview = build_preview(&connection, &path, &validated.import)?;
    preview.backup_file_count = validated.manifest.assets.len();
    preview.backup_total_bytes = validated.total_bytes;
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
        merge_import(&mut connection, import)
    };
    if result.is_ok() {
        crate::tray::update_task_badge(&app);
    }
    result
}

#[tauri::command]
pub fn confirm_full_backup_import(
    path: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<ImportResult, String> {
    let token = Uuid::new_v4();
    let app_data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取附件目录失败：{error}"))?;
    let asset_root = app_data_root.join("note-assets");
    fs::create_dir_all(&asset_root).map_err(|error| format!("创建附件目录失败：{error}"))?;
    let staging_root = asset_root.join(format!(".backup-import-{token}"));
    let rollback_root = asset_root.join(format!(".backup-rollback-{token}"));
    let database_snapshot = app_data_root.join(format!(".backup-database-{token}.sqlite3"));

    let validated = match read_full_backup_archive(Path::new(&path), Some(&staging_root)) {
        Ok(validated) => validated,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
    };
    let active_attachments = validated
        .import
        .note_attachments
        .iter()
        .filter(|attachment| attachment.deleted_at.is_none())
        .cloned()
        .collect::<Vec<_>>();

    let restore_result = (|| {
        let local_attachments = {
            let connection = lock_database(&database)?;
            backup_connection(&connection, &database_snapshot)?;
            note_attachment_sync::build_backup_document(&connection, now_millis())?.attachments
        };
        let restore_uuids = restore_candidate_uuids(&local_attachments, &active_attachments);
        let installed =
            install_backup_assets(&asset_root, &staging_root, &rollback_root, &restore_uuids)?;
        let database_result: Result<ImportResult, String> = (|| {
            let mut connection = lock_database(&database)?;
            let mut result = merge_import(&mut connection, validated.import)?;
            set_restored_attachment_paths(&connection, &active_attachments, &restore_uuids)?;
            result.restored_file_count = restore_uuids
                .iter()
                .filter_map(|uuid| active_attachments.iter().find(|item| &item.uuid == uuid))
                .map(|attachment| if attachment.kind == "image" { 2 } else { 1 })
                .sum();
            Ok(result)
        })();
        match database_result {
            Ok(result) => {
                let _ = fs::remove_dir_all(&rollback_root);
                Ok(result)
            }
            Err(error) => {
                let asset_rollback_error =
                    rollback_installed_assets(&asset_root, &rollback_root, &installed).err();
                let restore_error = lock_database(&database)
                    .and_then(|mut connection| {
                        restore_connection(&mut connection, &database_snapshot)
                    })
                    .err();
                let mut message = error;
                if let Some(rollback_error) = asset_rollback_error {
                    message.push_str(&format!("；附件回滚失败：{rollback_error}"));
                }
                if let Some(rollback_error) = restore_error {
                    message.push_str(&format!("；数据库回滚失败：{rollback_error}"));
                }
                Err(message)
            }
        }
    })();

    let _ = fs::remove_dir_all(&staging_root);
    let _ = fs::remove_dir_all(&rollback_root);
    let _ = fs::remove_file(&database_snapshot);
    if restore_result.is_ok() {
        crate::tray::update_task_badge(&app);
    }
    restore_result
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

fn restore_connection(destination: &mut Connection, path: &Path) -> Result<(), String> {
    let source =
        Connection::open(path).map_err(|error| format!("打开数据库回滚点失败：{error}"))?;
    let backup = Backup::new(&source, destination)
        .map_err(|error| format!("初始化数据库回滚失败：{error}"))?;
    backup
        .run_to_completion(32, std::time::Duration::from_millis(10), None)
        .map_err(|error| format!("数据库回滚失败：{error}"))
}

fn restore_candidate_uuids(
    local: &[SyncNoteAttachment],
    imported: &[SyncNoteAttachment],
) -> HashSet<String> {
    let local_by_uuid = local
        .iter()
        .map(|attachment| (&attachment.uuid, attachment))
        .collect::<HashMap<_, _>>();
    imported
        .iter()
        .filter(|attachment| {
            local_by_uuid
                .get(&attachment.uuid)
                .is_none_or(|local_attachment| {
                    note_attachment_sync::compare_attachments(attachment, local_attachment).is_ge()
                })
        })
        .map(|attachment| attachment.uuid.clone())
        .collect()
}

fn install_backup_assets(
    asset_root: &Path,
    staging_root: &Path,
    rollback_root: &Path,
    restore_uuids: &HashSet<String>,
) -> Result<Vec<InstalledAsset>, String> {
    fs::create_dir_all(rollback_root).map_err(|error| format!("创建附件回滚目录失败：{error}"))?;
    let mut installed = Vec::new();
    for uuid in restore_uuids {
        if Uuid::parse_str(uuid).is_err() {
            let _ = rollback_installed_assets(asset_root, rollback_root, &installed);
            return Err("恢复附件 UUID 无效".to_string());
        }
        let staged = staging_root.join(uuid);
        let final_path = asset_root.join(uuid);
        let previous = rollback_root.join(uuid);
        if !staged.is_dir() {
            let _ = rollback_installed_assets(asset_root, rollback_root, &installed);
            return Err(format!("完整备份缺少已校验附件目录：{uuid}"));
        }
        let had_previous = final_path.exists();
        if had_previous {
            if let Err(error) = fs::rename(&final_path, &previous) {
                let _ = rollback_installed_assets(asset_root, rollback_root, &installed);
                return Err(format!("建立附件回滚点失败：{error}"));
            }
        }
        if let Err(error) = fs::rename(&staged, &final_path) {
            if had_previous {
                let _ = fs::rename(&previous, &final_path);
            }
            let _ = rollback_installed_assets(asset_root, rollback_root, &installed);
            return Err(format!("原子恢复附件失败：{error}"));
        }
        installed.push(InstalledAsset {
            uuid: uuid.clone(),
            had_previous,
        });
    }
    Ok(installed)
}

fn rollback_installed_assets(
    asset_root: &Path,
    rollback_root: &Path,
    installed: &[InstalledAsset],
) -> Result<(), String> {
    let mut errors = Vec::new();
    for item in installed.iter().rev() {
        let final_path = asset_root.join(&item.uuid);
        let previous = rollback_root.join(&item.uuid);
        if final_path.exists() {
            if let Err(error) = fs::remove_dir_all(&final_path) {
                errors.push(format!("删除 {} 失败：{error}", item.uuid));
                continue;
            }
        }
        if item.had_previous {
            if let Err(error) = fs::rename(previous, final_path) {
                errors.push(format!("恢复 {} 失败：{error}", item.uuid));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

fn set_restored_attachment_paths(
    connection: &Connection,
    attachments: &[SyncNoteAttachment],
    restore_uuids: &HashSet<String>,
) -> Result<(), String> {
    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    for attachment in attachments
        .iter()
        .filter(|attachment| restore_uuids.contains(&attachment.uuid))
    {
        let original = format!("note-assets/{}/original", attachment.uuid);
        let preview = (attachment.kind == "image")
            .then(|| format!("note-assets/{}/preview.jpg", attachment.uuid));
        transaction
            .execute(
                "UPDATE note_attachments
                 SET local_original_path = ?1, local_preview_path = ?2,
                     transfer_state = 'synced', transfer_error = NULL, remote_uploaded = 1
                 WHERE uuid = ?3 AND deleted_at IS NULL",
                params![original, preview, attachment.uuid],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
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
    validate_import_mode(import, false)
}

fn validate_complete_import(import: &TodoExport) -> Result<(), String> {
    validate_import_mode(import, true)
}

fn validate_import_mode(import: &TodoExport, expect_attachment_files: bool) -> Result<(), String> {
    if import.format_version > FORMAT_VERSION {
        return Err(format!(
            "导入文件版本 {} 高于当前支持的版本 {}",
            import.format_version, FORMAT_VERSION
        ));
    }
    if import.format_version == 0 {
        return Err("导入文件缺少有效的 format_version".to_string());
    }
    if import.attachment_files_included != expect_attachment_files {
        if expect_attachment_files {
            return Err("完整备份必须声明包含附件文件".to_string());
        }
        return Err("普通 JSON 不能声明包含附件文件，请使用 .eggdone-backup".to_string());
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
        if !matches!(todo.priority, 0 | 1) {
            return Err("导入文件包含无效任务重要级别".to_string());
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
    note_sync::validate_notes(&import.notes)?;
    note_attachment_sync::validate_document(&NoteAttachmentSyncDocument {
        format_version: note_attachment_sync::NOTE_ATTACHMENT_SYNC_FORMAT_VERSION,
        device_id: "00000000-0000-4000-8000-000000000001".to_string(),
        generated_at: import.exported_at,
        attachments: import.note_attachments.clone(),
    })?;
    if expect_attachment_files {
        let notes = import
            .notes
            .iter()
            .map(|note| (&note.uuid, note.deleted_at))
            .collect::<HashMap<_, _>>();
        for attachment in import
            .note_attachments
            .iter()
            .filter(|attachment| attachment.deleted_at.is_none())
        {
            match notes.get(&attachment.note_uuid) {
                Some(None) => {}
                Some(Some(_)) => {
                    return Err(format!(
                        "完整备份的活动附件引用了已删除便签：{}",
                        attachment.display_name
                    ))
                }
                None => {
                    return Err(format!(
                        "完整备份的附件缺少所属便签：{}",
                        attachment.display_name
                    ))
                }
            }
        }
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
            SELECT uuid, title, group_uuid, completed, pinned, priority, sort_order, created_at, updated_at,
                   completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                   repeat_rule, repeat_next_due_date, repeat_series_uuid, note
            FROM todos
            ORDER BY completed ASC, pinned DESC, sort_order ASC, created_at DESC, uuid DESC
            ",
        )
        .map_err(database_error)?;
    let todos = statement
        .query_map([], map_transfer_todo)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?
        .into_iter()
        .map(canonicalize_repeat_schedule)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(todos)
}

fn build_preview(
    connection: &Connection,
    path: &Path,
    import: &TodoExport,
) -> Result<ImportPreview, String> {
    let local_versions = local_versions(connection)?;
    let mut added = 0;
    let mut updated = 0;
    let mut unchanged = 0;

    for todo in &import.todos {
        match local_versions.get(&todo.uuid) {
            None => added += 1,
            Some(local_updated_at) if todo.updated_at > *local_updated_at => updated += 1,
            Some(_) => unchanged += 1,
        }
    }
    let (note_added, note_updated, note_unchanged) = count_note_changes(connection, &import.notes)?;
    let (attachment_added, attachment_updated, attachment_unchanged) =
        count_attachment_changes(connection, &import.note_attachments)?;

    Ok(ImportPreview {
        path: path.to_string_lossy().into_owned(),
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("EggDone JSON")
            .to_string(),
        total: import.todos.len(),
        added,
        updated,
        unchanged,
        note_total: import.notes.len(),
        note_added,
        note_updated,
        note_unchanged,
        attachment_total: import.note_attachments.len(),
        attachment_added,
        attachment_updated,
        attachment_unchanged,
        attachment_files_included: import.attachment_files_included,
        backup_file_count: 0,
        backup_total_bytes: 0,
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
        note_added: 0,
        note_updated: 0,
        note_unchanged: 0,
        attachment_added: 0,
        attachment_updated: 0,
        attachment_unchanged: 0,
        restored_file_count: 0,
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

fn merge_import(connection: &mut Connection, import: TodoExport) -> Result<ImportResult, String> {
    let note_changes = count_note_changes(connection, &import.notes)?;
    let attachment_changes = count_attachment_changes(connection, &import.note_attachments)?;
    let mut result = merge_transfer(connection, &import.groups, &import.todos)?;
    result.note_added = note_changes.0;
    result.note_updated = note_changes.1;
    result.note_unchanged = note_changes.2;
    result.attachment_added = attachment_changes.0;
    result.attachment_updated = attachment_changes.1;
    result.attachment_unchanged = attachment_changes.2;
    if !import.notes.is_empty() {
        let remote = NoteSyncDocument {
            format_version: note_sync::NOTE_SYNC_FORMAT_VERSION,
            device_id: device_id(connection).map_err(database_error)?,
            generated_at: import.exported_at,
            notes: import.notes,
        };
        note_sync::merge_remote_document(connection, &remote, now_millis())?;
    }
    if !import.note_attachments.is_empty() {
        let remote = NoteAttachmentSyncDocument {
            format_version: note_attachment_sync::NOTE_ATTACHMENT_SYNC_FORMAT_VERSION,
            device_id: device_id(connection).map_err(database_error)?,
            generated_at: import.exported_at,
            attachments: import.note_attachments,
        };
        note_attachment_sync::merge_remote_document(connection, &remote, now_millis())?;
    }
    Ok(result)
}

fn count_attachment_changes(
    connection: &Connection,
    imported: &[SyncNoteAttachment],
) -> Result<(usize, usize, usize), String> {
    let local = note_attachment_sync::build_document(connection, now_millis())?;
    let local_attachments = local
        .attachments
        .into_iter()
        .map(|attachment| (attachment.uuid.clone(), attachment))
        .collect::<HashMap<_, _>>();
    let mut added = 0;
    let mut updated = 0;
    let mut unchanged = 0;
    for attachment in imported {
        match local_attachments.get(&attachment.uuid) {
            None => added += 1,
            Some(local_attachment)
                if note_attachment_sync::compare_attachments(attachment, local_attachment)
                    .is_gt() =>
            {
                updated += 1;
            }
            Some(_) => unchanged += 1,
        }
    }
    Ok((added, updated, unchanged))
}

fn count_note_changes(
    connection: &Connection,
    imported: &[SyncNote],
) -> Result<(usize, usize, usize), String> {
    let local = note_sync::build_document(connection, now_millis())?;
    let local_notes = local
        .notes
        .into_iter()
        .map(|note| (note.uuid.clone(), note))
        .collect::<HashMap<_, _>>();
    let mut added = 0;
    let mut updated = 0;
    let mut unchanged = 0;
    for note in imported {
        match local_notes.get(&note.uuid) {
            None => added += 1,
            Some(local_note) if note_sync::compare_notes(note, local_note).is_gt() => updated += 1,
            Some(_) => unchanged += 1,
        }
    }
    Ok((added, updated, unchanged))
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
    let (due_date, due_at) = schedule_for_local_storage(todo, None)?;
    connection
        .execute(
            "
            INSERT INTO todos (
                uuid, title, group_uuid, completed, pinned, priority, sort_order, created_at, updated_at,
                completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, note, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ",
            params![
                todo.uuid,
                todo.title.trim(),
                todo.group_uuid,
                todo.completed,
                todo.pinned,
                todo.priority,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                todo.archived_at,
                due_date,
                due_at,
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
    let existing_due_at = connection
        .query_row(
            "SELECT due_at FROM todos WHERE uuid = ?1",
            params![todo.uuid],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(database_error)?
        .flatten();
    let (due_date, due_at) = schedule_for_local_storage(todo, existing_due_at)?;
    connection
        .execute(
            "
            UPDATE todos
            SET title = ?1, completed = ?2, pinned = ?3, priority = ?4, sort_order = ?5,
                created_at = ?6, updated_at = ?7, completed_at = ?8,
                deleted_at = ?9, archived_at = ?10, due_date = ?11, due_at = ?12, reminder_at = ?13,
                repeat_rule = ?14, repeat_next_due_date = ?15,
                repeat_series_uuid = ?16, note = ?17, group_uuid = ?18, updated_by = ?19
            WHERE uuid = ?20
            ",
            params![
                todo.title.trim(),
                todo.completed,
                todo.pinned,
                todo.priority,
                todo.sort_order,
                todo.created_at,
                todo.updated_at,
                todo.completed_at,
                todo.deleted_at,
                todo.archived_at,
                due_date,
                due_at,
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
        priority: row.get(5)?,
        sort_order: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
        deleted_at: row.get(10)?,
        archived_at: row.get(11)?,
        due_date: row.get(12)?,
        due_at: row.get(13)?,
        reminder_at: row.get(14)?,
        repeat_rule: row.get(15)?,
        repeat_next_due_date: row.get(16)?,
        repeat_series_uuid: row.get(17)?,
        note: row.get(18)?,
    })
}

fn canonicalize_repeat_schedule(mut todo: TransferTodo) -> Result<TransferTodo, String> {
    if todo.repeat_rule.is_some() && todo.due_date.is_none() {
        let timestamp = todo
            .due_at
            .ok_or_else(|| "重复任务缺少到期时间".to_string())?;
        todo.due_date = Some(local_date_from_timestamp(timestamp)?);
        todo.due_at = None;
    }
    Ok(todo)
}

fn schedule_for_local_storage(
    todo: &TransferTodo,
    existing_due_at: Option<i64>,
) -> Result<(Option<String>, Option<i64>), String> {
    if todo.repeat_rule.is_none() {
        return Ok((todo.due_date.clone(), todo.due_at));
    }

    let date = todo
        .due_date
        .as_deref()
        .ok_or_else(|| "导入的重复任务缺少到期日期".to_string())?;
    Ok((None, Some(timestamp_for_local_date(date, existing_due_at)?)))
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
    use std::io::Read;

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
            priority: 0,
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

    fn note(uuid: &str, title: &str, updated_at: i64) -> SyncNote {
        SyncNote {
            uuid: uuid.to_string(),
            title: title.to_string(),
            content: "backup content".to_string(),
            color: "yellow".to_string(),
            pinned: true,
            created_at: 1,
            updated_at,
            deleted_at: None,
            updated_by: "00000000-0000-4000-8000-000000000099".to_string(),
        }
    }

    fn attachment(uuid: &str, note_uuid: &str, updated_at: i64) -> SyncNoteAttachment {
        SyncNoteAttachment {
            uuid: uuid.to_string(),
            note_uuid: note_uuid.to_string(),
            kind: "file".to_string(),
            display_name: "backup.md".to_string(),
            mime_type: "text/markdown".to_string(),
            byte_size: 7,
            sha256: "a".repeat(64),
            preview_mime_type: None,
            preview_byte_size: None,
            preview_sha256: None,
            width: None,
            height: None,
            sort_order: 0,
            created_at: 1,
            updated_at,
            deleted_at: None,
            updated_by: "00000000-0000-4000-8000-000000000099".to_string(),
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
        let exported_note = note("00000000-0000-4000-8000-000000000003", "note", 4);
        let exported_attachment = attachment(
            "00000000-0000-4000-8000-000000000004",
            &exported_note.uuid,
            5,
        );
        merge_transfer(
            &mut source,
            std::slice::from_ref(&work),
            &[active.clone(), deleted.clone()],
        )
        .unwrap();
        note_sync::merge_remote_document(
            &mut source,
            &NoteSyncDocument {
                format_version: note_sync::NOTE_SYNC_FORMAT_VERSION,
                device_id: "00000000-0000-4000-8000-000000000099".to_string(),
                generated_at: 4,
                notes: vec![exported_note.clone()],
            },
            4,
        )
        .unwrap();
        note_attachment_sync::merge_remote_document(
            &mut source,
            &NoteAttachmentSyncDocument {
                format_version: note_attachment_sync::NOTE_ATTACHMENT_SYNC_FORMAT_VERSION,
                device_id: "00000000-0000-4000-8000-000000000099".to_string(),
                generated_at: 5,
                attachments: vec![exported_attachment.clone()],
            },
            5,
        )
        .unwrap();

        let export = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 10,
            groups: read_all_groups(&source).unwrap(),
            todos: read_all_todos(&source).unwrap(),
            notes: note_sync::build_document(&source, 10).unwrap().notes,
            note_attachments: note_attachment_sync::build_document(&source, 10)
                .unwrap()
                .attachments,
            attachment_files_included: false,
        };
        let json = serde_json::to_string(&export).unwrap();
        let exported: TodoExport = serde_json::from_str(&json).unwrap();
        validate_import(&exported).unwrap();
        let mut destination = connection();
        let result = merge_import(&mut destination, exported).unwrap();

        assert_eq!(result.added, 2);
        assert_eq!(result.note_added, 1);
        assert_eq!(result.attachment_added, 1);
        let (stored_due_date, stored_due_at): (Option<String>, Option<i64>) = destination
            .query_row(
                "SELECT due_date, due_at FROM todos WHERE uuid = ?1",
                params![active.uuid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_due_date, None);
        assert_eq!(
            stored_due_at,
            Some(timestamp_for_local_date("2026-06-10", None).unwrap())
        );
        assert_eq!(read_all_groups(&destination).unwrap(), vec![work]);
        assert_eq!(read_all_todos(&destination).unwrap(), vec![active, deleted]);
        assert_eq!(
            note_sync::build_document(&destination, 10).unwrap().notes,
            vec![exported_note]
        );
        assert_eq!(
            note_attachment_sync::build_document(&destination, 10)
                .unwrap()
                .attachments,
            vec![exported_attachment]
        );
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
            notes: vec![],
            note_attachments: vec![],
            attachment_files_included: false,
        };
        assert!(validate_import(&future).is_err());

        let duplicated = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 1,
            groups: vec![],
            todos: vec![shared.clone(), shared],
            notes: vec![],
            note_attachments: vec![],
            attachment_files_included: false,
        };
        assert!(validate_import(&duplicated).is_err());

        let falsely_complete = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 1,
            groups: vec![],
            todos: vec![],
            notes: vec![],
            note_attachments: vec![],
            attachment_files_included: true,
        };
        assert!(validate_import(&falsely_complete).is_err());
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
        assert_eq!(import.todos[0].priority, 0);
        assert_eq!(import.todos[0].note, None);
        assert_eq!(import.todos[0].archived_at, None);
        assert_eq!(import.todos[0].due_date, None);
        assert!(import.notes.is_empty());
        assert!(import.note_attachments.is_empty());
        assert!(!import.attachment_files_included);
        validate_import(&import).unwrap();
    }

    #[test]
    fn note_import_uses_sync_conflict_tiebreakers() {
        let mut connection = connection();
        let uuid = "00000000-0000-4000-8000-000000000007";
        let local = note(uuid, "local", 10);
        note_sync::merge_remote_document(
            &mut connection,
            &NoteSyncDocument {
                format_version: note_sync::NOTE_SYNC_FORMAT_VERSION,
                device_id: local.updated_by.clone(),
                generated_at: 10,
                notes: vec![local],
            },
            10,
        )
        .unwrap();
        let mut imported = note(uuid, "imported", 10);
        imported.updated_by = "00000000-0000-4000-8000-000000000100".to_string();
        let result = merge_import(
            &mut connection,
            TodoExport {
                format_version: FORMAT_VERSION,
                exported_at: 11,
                groups: vec![],
                todos: vec![],
                notes: vec![imported.clone()],
                note_attachments: vec![],
                attachment_files_included: false,
            },
        )
        .unwrap();

        assert_eq!(result.note_updated, 1);
        assert_eq!(
            note_sync::build_document(&connection, 11).unwrap().notes,
            vec![imported]
        );
    }

    #[test]
    fn full_backup_archive_uses_fixed_paths_and_matching_manifest_hashes() {
        let directory = std::env::temp_dir().join(format!("eggdone-backup-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sample.eggdone-backup");
        let data = br#"{"attachment_files_included":true}"#;
        let asset = b"attachment bytes".to_vec();
        let asset_path = directory.join("original");
        fs::write(&asset_path, &asset).unwrap();
        let entry = backup_manifest_entry(
            "note-assets/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/original".to_string(),
            &asset,
        );
        let manifest = serde_json::to_vec(&BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            created_at: 1,
            data: backup_manifest_entry("data.json".to_string(), data),
            assets: vec![entry.clone()],
        })
        .unwrap();

        write_full_backup_archive(&path, &manifest, data, &[(entry, asset_path)]).unwrap();

        let mut archive = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 3);
        let mut restored = Vec::new();
        archive
            .by_name("note-assets/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/original")
            .unwrap()
            .read_to_end(&mut restored)
            .unwrap();
        assert_eq!(restored, asset);
        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("data.json").is_ok());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn validates_and_extracts_a_complete_backup() {
        let directory = std::env::temp_dir().join(format!("eggdone-restore-{}", Uuid::new_v4()));
        let extraction = directory.join("extracted");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sample.eggdone-backup");
        let note_uuid = "00000000-0000-4000-8000-000000000020";
        let attachment_uuid = "00000000-0000-4000-8000-000000000021";
        let asset = b"backup!".to_vec();
        let asset_sha256 = format!("{:x}", Sha256::digest(&asset));
        let mut exported_attachment = attachment(attachment_uuid, note_uuid, 5);
        exported_attachment.byte_size = asset.len() as i64;
        exported_attachment.sha256 = asset_sha256.clone();
        let export = TodoExport {
            format_version: FORMAT_VERSION,
            exported_at: 10,
            groups: vec![],
            todos: vec![],
            notes: vec![note(note_uuid, "restore", 4)],
            note_attachments: vec![exported_attachment],
            attachment_files_included: true,
        };
        let data = serde_json::to_vec(&export).unwrap();
        let asset_path = directory.join("original");
        fs::write(&asset_path, &asset).unwrap();
        let entry = BackupManifestEntry {
            path: format!("note-assets/{attachment_uuid}/original"),
            byte_size: asset.len() as u64,
            sha256: asset_sha256,
        };
        let manifest = serde_json::to_vec(&BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            created_at: 10,
            data: backup_manifest_entry("data.json".to_string(), &data),
            assets: vec![entry.clone()],
        })
        .unwrap();
        write_full_backup_archive(&path, &manifest, &data, &[(entry, asset_path)]).unwrap();

        let validated = read_full_backup_archive(&path, Some(&extraction)).unwrap();
        assert!(validated.import.attachment_files_included);
        assert_eq!(validated.manifest.assets.len(), 1);
        assert_eq!(
            fs::read(extraction.join(attachment_uuid).join("original")).unwrap(),
            asset
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_unsafe_backup_paths_and_invalid_hashes() {
        assert!(validate_backup_entry_path("../data.json", false).is_err());
        assert!(validate_backup_entry_path("note-assets\\bad", false).is_err());
        let entry = BackupManifestEntry {
            path: "data.json".to_string(),
            byte_size: 4,
            sha256: "0".repeat(64),
        };
        assert!(verify_manifest_bytes(&entry, b"data").is_err());
    }
}
