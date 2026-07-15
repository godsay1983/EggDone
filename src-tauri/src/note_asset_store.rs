use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{
    DynamicImage, ExtendedColorType, GenericImageView, ImageFormat, ImageReader, RgbImage,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::note_attachments::{
    NoteAttachment, FILE_UPLOAD_MAX_BYTES, IMAGE_UPLOAD_MAX_BYTES, PREVIEW_MAX_BYTES,
};

const ASSET_DIRECTORY: &str = "note-assets";
const ORIGINAL_FILE_NAME: &str = "original";
const PREVIEW_FILE_NAME: &str = "preview.jpg";
const PREVIEW_MAX_EDGE: u32 = 512;
const PREVIEW_JPEG_QUALITY: u8 = 85;
const MAX_IMAGE_EDGE: u32 = 20_000;
const MAX_IMAGE_PIXELS: u64 = 80_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedImageAsset {
    pub display_name: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub preview_mime_type: String,
    pub preview_byte_size: i64,
    pub preview_sha256: String,
    pub width: i64,
    pub height: i64,
    pub local_original_path: String,
    pub local_preview_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedFileAsset {
    pub display_name: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub local_original_path: String,
}

pub(crate) struct NoteAssetStore {
    app_data_root: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteAttachmentCacheStats {
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub protected_bytes: u64,
    pub file_count: u64,
    pub reclaimable_file_count: u64,
    pub protected_file_count: u64,
    pub pending_count: u64,
}

impl NoteAssetStore {
    pub(crate) fn from_app(app: &AppHandle) -> Result<Self, String> {
        let app_data_root = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("获取附件目录失败：{error}"))?;
        Ok(Self { app_data_root })
    }

    #[cfg(test)]
    fn for_root(app_data_root: PathBuf) -> Self {
        Self { app_data_root }
    }

    pub(crate) fn import_image(
        &self,
        source: &Path,
        attachment_uuid: &str,
    ) -> Result<PreparedImageAsset, String> {
        validate_uuid(attachment_uuid)?;
        let display_name = display_name(source)?;
        let asset_root = self.app_data_root.join(ASSET_DIRECTORY);
        fs::create_dir_all(&asset_root).map_err(|error| format!("创建附件目录失败：{error}"))?;

        let final_directory = asset_root.join(attachment_uuid);
        if final_directory.exists() {
            return Err("附件文件已存在，请使用新的附件 UUID".to_string());
        }
        let staging_directory =
            asset_root.join(format!(".staging-{attachment_uuid}-{}", Uuid::new_v4()));
        fs::create_dir(&staging_directory)
            .map_err(|error| format!("创建附件临时目录失败：{error}"))?;

        let result = self.prepare_image_in_staging(
            source,
            attachment_uuid,
            &display_name,
            &staging_directory,
            &final_directory,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging_directory);
        }
        result
    }

    pub(crate) fn import_image_bytes(
        &self,
        bytes: &[u8],
        display_name: &str,
        attachment_uuid: &str,
    ) -> Result<PreparedImageAsset, String> {
        validate_uuid(attachment_uuid)?;
        validate_display_name(display_name)?;
        if bytes.is_empty() {
            return Err("图片文件为空".to_string());
        }
        if bytes.len() as i64 > IMAGE_UPLOAD_MAX_BYTES {
            return Err("图片不能超过 10 MiB".to_string());
        }

        let asset_root = self.app_data_root.join(ASSET_DIRECTORY);
        fs::create_dir_all(&asset_root).map_err(|error| format!("创建附件目录失败：{error}"))?;
        let source = asset_root.join(format!(".import-{attachment_uuid}-{}", Uuid::new_v4()));
        write_new_file(&source, bytes)?;
        let result = self
            .import_image(&source, attachment_uuid)
            .map(|mut prepared| {
                prepared.display_name = display_name.to_string();
                prepared
            });
        let _ = fs::remove_file(source);
        result
    }

    pub(crate) fn import_file_bytes(
        &self,
        bytes: &[u8],
        display_name: &str,
        attachment_uuid: &str,
    ) -> Result<PreparedFileAsset, String> {
        validate_uuid(attachment_uuid)?;
        if bytes.is_empty() {
            return Err("附件文件为空".to_string());
        }
        if bytes.len() as i64 > FILE_UPLOAD_MAX_BYTES {
            return Err("附件不能超过 20 MiB".to_string());
        }
        let display_name = sanitize_file_display_name(display_name)?;
        let mime_type = detect_file_mime_type(&display_name, bytes)?.to_string();

        let asset_root = self.app_data_root.join(ASSET_DIRECTORY);
        fs::create_dir_all(&asset_root).map_err(|error| format!("创建附件目录失败：{error}"))?;
        let final_directory = asset_root.join(attachment_uuid);
        if final_directory.exists() {
            return Err("附件文件已存在，请使用新的附件 UUID".to_string());
        }
        let staging_directory =
            asset_root.join(format!(".staging-{attachment_uuid}-{}", Uuid::new_v4()));
        fs::create_dir(&staging_directory)
            .map_err(|error| format!("创建附件临时目录失败：{error}"))?;
        let staging_original = staging_directory.join(ORIGINAL_FILE_NAME);
        let result = (|| {
            write_new_file(&staging_original, bytes)?;
            let (byte_size, sha256) = hash_file(&staging_original, FILE_UPLOAD_MAX_BYTES)?;
            fs::rename(&staging_directory, &final_directory)
                .map_err(|error| format!("保存附件文件失败：{error}"))?;
            Ok(PreparedFileAsset {
                display_name,
                mime_type,
                byte_size,
                sha256,
                local_original_path: relative_asset_path(attachment_uuid, ORIGINAL_FILE_NAME),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging_directory);
        }
        result
    }

    pub(crate) fn prepare_file_for_open(
        &self,
        attachment_uuid: &str,
        display_name: &str,
    ) -> Result<PathBuf, String> {
        validate_uuid(attachment_uuid)?;
        let safe_name = sanitize_file_display_name(display_name)?;
        if safe_name != display_name {
            return Err("附件文件名包含不安全字符".to_string());
        }
        let directory = self
            .app_data_root
            .join(ASSET_DIRECTORY)
            .join(attachment_uuid);
        let source = directory.join(ORIGINAL_FILE_NAME);
        if !source.is_file() {
            return Err("附件文件不存在".to_string());
        }
        let destination = directory.join(format!("open-{safe_name}"));
        if !destination.is_file() {
            fs::copy(&source, &destination)
                .map_err(|error| format!("准备打开附件失败：{error}"))?;
        }
        Ok(destination)
    }

    pub(crate) fn verify_local_file(
        &self,
        relative_path: &str,
        expected_size: i64,
        expected_sha256: &str,
    ) -> Result<bool, String> {
        if expected_size <= 0 || !is_sha256(expected_sha256) {
            return Err("附件校验参数无效".to_string());
        }
        let path = self.resolve_relative_path(relative_path)?;
        if !path.is_file() {
            return Ok(false);
        }
        let (size, sha256) = hash_file(&path, i64::MAX)?;
        Ok(size == expected_size && sha256 == expected_sha256)
    }

    pub(crate) fn read_asset_file(
        &self,
        attachment_uuid: &str,
        file_name: &str,
        expected_size: i64,
        expected_sha256: &str,
    ) -> Result<Vec<u8>, String> {
        let relative_path = validated_asset_relative_path(attachment_uuid, file_name)?;
        if !self.verify_local_file(&relative_path, expected_size, expected_sha256)? {
            return Err("本地附件缺失或校验失败，请重新选择文件".to_string());
        }
        fs::read(self.resolve_relative_path(&relative_path)?)
            .map_err(|error| format!("读取本地附件失败：{error}"))
    }

    pub(crate) fn verified_asset_path(
        &self,
        attachment_uuid: &str,
        file_name: &str,
        expected_size: i64,
        expected_sha256: &str,
    ) -> Result<PathBuf, String> {
        let relative_path = validated_asset_relative_path(attachment_uuid, file_name)?;
        if !self.verify_local_file(&relative_path, expected_size, expected_sha256)? {
            return Err("本地附件缺失或校验失败，请重新选择文件".to_string());
        }
        self.resolve_relative_path(&relative_path)
    }

    pub(crate) fn write_downloaded_asset(
        &self,
        attachment_uuid: &str,
        file_name: &str,
        bytes: &[u8],
        expected_size: i64,
        expected_sha256: &str,
    ) -> Result<String, String> {
        let relative_path = validated_asset_relative_path(attachment_uuid, file_name)?;
        if bytes.len() as i64 != expected_size || sha256_bytes(bytes) != expected_sha256 {
            return Err("下载附件校验失败，未写入本地缓存".to_string());
        }
        let final_path = self.resolve_relative_path(&relative_path)?;
        if self.verify_local_file(&relative_path, expected_size, expected_sha256)? {
            return Ok(relative_path);
        }
        let directory = final_path
            .parent()
            .ok_or_else(|| "附件目标目录无效".to_string())?;
        fs::create_dir_all(directory).map_err(|error| format!("创建附件目录失败：{error}"))?;
        let staging_path = directory.join(format!(".{file_name}.download-{}", Uuid::new_v4()));
        let result = (|| {
            write_new_file(&staging_path, bytes)?;
            let (size, sha256) = hash_file(&staging_path, expected_size)?;
            if size != expected_size || sha256 != expected_sha256 {
                return Err("下载附件临时文件校验失败".to_string());
            }
            if final_path.exists() {
                fs::remove_file(&final_path)
                    .map_err(|error| format!("替换损坏附件失败：{error}"))?;
            }
            fs::rename(&staging_path, &final_path)
                .map_err(|error| format!("保存下载附件失败：{error}"))?;
            Ok(relative_path.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_file(staging_path);
        }
        result
    }

    pub(crate) fn delete_asset(&self, attachment_uuid: &str) -> Result<(), String> {
        validate_uuid(attachment_uuid)?;
        let directory = self
            .app_data_root
            .join(ASSET_DIRECTORY)
            .join(attachment_uuid);
        if !directory.exists() {
            return Ok(());
        }
        fs::remove_dir_all(directory).map_err(|error| format!("删除附件文件失败：{error}"))
    }

    pub(crate) fn cache_stats(
        &self,
        attachments: &[NoteAttachment],
    ) -> Result<NoteAttachmentCacheStats, String> {
        let mut stats = NoteAttachmentCacheStats::default();
        let mut seen = HashSet::new();
        for attachment in attachments {
            for relative_path in [
                attachment.local_original_path.as_deref(),
                attachment.local_preview_path.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if !seen.insert(relative_path.to_string()) {
                    continue;
                }
                let path = self.resolve_relative_path(relative_path)?;
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(format!("读取附件缓存信息失败：{error}")),
                };
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err("附件缓存包含无效文件".to_string());
                }
                stats.total_bytes = stats.total_bytes.saturating_add(metadata.len());
                stats.file_count += 1;
                if attachment.remote_uploaded {
                    stats.reclaimable_bytes =
                        stats.reclaimable_bytes.saturating_add(metadata.len());
                    stats.reclaimable_file_count += 1;
                } else {
                    stats.protected_bytes = stats.protected_bytes.saturating_add(metadata.len());
                    stats.protected_file_count += 1;
                }
            }
        }
        Ok(stats)
    }

    pub(crate) fn clear_reclaimable_cache(
        &self,
        attachments: &[NoteAttachment],
    ) -> Result<(), String> {
        let mut seen = HashSet::new();
        let mut directories = HashSet::new();
        for attachment in attachments
            .iter()
            .filter(|attachment| attachment.remote_uploaded)
        {
            for relative_path in [
                attachment.local_original_path.as_deref(),
                attachment.local_preview_path.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if !seen.insert(relative_path.to_string()) {
                    continue;
                }
                let path = self.resolve_relative_path(relative_path)?;
                if let Some(parent) = path.parent() {
                    directories.insert(parent.to_path_buf());
                }
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                        return Err("附件缓存包含无效文件".to_string());
                    }
                    Ok(_) => fs::remove_file(&path)
                        .map_err(|error| format!("清理附件缓存失败：{error}"))?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("读取附件缓存信息失败：{error}")),
                }
            }
        }
        for directory in directories {
            if let Ok(entries) = fs::read_dir(&directory) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name();
                    if file_name.to_string_lossy().starts_with("open-") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
            match fs::remove_dir(&directory) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => return Err(format!("清理附件缓存目录失败：{error}")),
            }
        }
        Ok(())
    }

    fn prepare_image_in_staging(
        &self,
        source: &Path,
        attachment_uuid: &str,
        display_name: &str,
        staging_directory: &Path,
        final_directory: &Path,
    ) -> Result<PreparedImageAsset, String> {
        let staging_original = staging_directory.join(ORIGINAL_FILE_NAME);
        let (byte_size, sha256) = copy_and_hash(source, &staging_original)?;
        let (image, format, width, height) = decode_supported_image(&staging_original)?;
        let preview = create_jpeg_preview(&image)?;
        if preview.len() as i64 > PREVIEW_MAX_BYTES {
            return Err("图片预览超过 2 MiB 限制".to_string());
        }
        let preview_sha256 = sha256_bytes(&preview);
        let staging_preview = staging_directory.join(PREVIEW_FILE_NAME);
        write_new_file(&staging_preview, &preview)?;

        fs::rename(staging_directory, final_directory)
            .map_err(|error| format!("保存附件文件失败：{error}"))?;

        Ok(PreparedImageAsset {
            display_name: display_name.to_string(),
            mime_type: mime_type(format).to_string(),
            byte_size,
            sha256,
            preview_mime_type: "image/jpeg".to_string(),
            preview_byte_size: preview.len() as i64,
            preview_sha256,
            width: i64::from(width),
            height: i64::from(height),
            local_original_path: relative_asset_path(attachment_uuid, ORIGINAL_FILE_NAME),
            local_preview_path: relative_asset_path(attachment_uuid, PREVIEW_FILE_NAME),
        })
    }

    fn resolve_relative_path(&self, relative_path: &str) -> Result<PathBuf, String> {
        let path = Path::new(relative_path);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err("附件本地路径无效".to_string());
        }
        let mut components = path.components();
        if components.next() != Some(Component::Normal(ASSET_DIRECTORY.as_ref())) {
            return Err("附件本地路径不属于附件目录".to_string());
        }
        Ok(self.app_data_root.join(path))
    }
}

fn copy_and_hash(source: &Path, destination: &Path) -> Result<(i64, String), String> {
    let metadata = source
        .metadata()
        .map_err(|error| format!("读取图片信息失败：{error}"))?;
    if !metadata.is_file() {
        return Err("选择的图片不是普通文件".to_string());
    }
    if metadata.len() == 0 {
        return Err("图片文件为空".to_string());
    }
    if metadata.len() > IMAGE_UPLOAD_MAX_BYTES as u64 {
        return Err("图片不能超过 10 MiB".to_string());
    }

    let source_file = File::open(source).map_err(|error| format!("打开图片失败：{error}"))?;
    let destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("创建附件文件失败：{error}"))?;
    let mut reader = BufReader::new(source_file);
    let mut writer = BufWriter::new(destination_file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_i64;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("读取图片失败：{error}"))?;
        if count == 0 {
            break;
        }
        total += count as i64;
        if total > IMAGE_UPLOAD_MAX_BYTES {
            return Err("图片不能超过 10 MiB".to_string());
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| format!("复制图片失败：{error}"))?;
        hasher.update(&buffer[..count]);
    }
    writer
        .flush()
        .map_err(|error| format!("保存图片失败：{error}"))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("同步图片文件失败：{error}"))?;
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn decode_supported_image(path: &Path) -> Result<(DynamicImage, ImageFormat, u32, u32), String> {
    let reader = ImageReader::open(path)
        .map_err(|error| format!("打开图片失败：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("识别图片格式失败：{error}"))?;
    let format = reader
        .format()
        .filter(|format| {
            matches!(
                format,
                ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
            )
        })
        .ok_or_else(|| "仅支持 JPEG、PNG 和 WebP 图片".to_string())?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| format!("读取图片尺寸失败：{error}"))?;
    validate_dimensions(width, height)?;

    let image = ImageReader::open(path)
        .map_err(|error| format!("打开图片失败：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("识别图片格式失败：{error}"))?
        .decode()
        .map_err(|error| format!("图片损坏或无法解码：{error}"))?;
    Ok((image, format, width, height))
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_IMAGE_EDGE
        || height > MAX_IMAGE_EDGE
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err("图片尺寸过大或无效".to_string());
    }
    Ok(())
}

fn create_jpeg_preview(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    let resized = if width > PREVIEW_MAX_EDGE || height > PREVIEW_MAX_EDGE {
        image.resize(PREVIEW_MAX_EDGE, PREVIEW_MAX_EDGE, FilterType::Lanczos3)
    } else {
        image.clone()
    };
    let rgba = resized.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (source, destination) in rgba.pixels().zip(rgb.pixels_mut()) {
        let alpha = u16::from(source[3]);
        for channel in 0..3 {
            let foreground = u16::from(source[channel]);
            destination[channel] = ((foreground * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
    }

    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, PREVIEW_JPEG_QUALITY)
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("生成图片预览失败：{error}"))?;
    Ok(output)
}

fn display_name(source: &Path) -> Result<String, String> {
    let value = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "图片文件名无效".to_string())?;
    validate_display_name(value)?;
    Ok(value.to_string())
}

fn validate_display_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.chars().count() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err("图片文件名无效".to_string());
    }
    Ok(())
}

pub(crate) fn validate_safe_file_metadata(
    display_name: &str,
    mime_type: &str,
) -> Result<(), String> {
    let sanitized = sanitize_file_display_name(display_name)?;
    if sanitized != display_name {
        return Err("附件文件名包含不安全字符".to_string());
    }
    let expected = mime_type_for_extension(file_extension(display_name)?)
        .ok_or_else(|| "不支持此附件类型".to_string())?;
    if mime_type != expected {
        return Err("附件扩展名与 MIME 类型不一致".to_string());
    }
    Ok(())
}

fn sanitize_file_display_name(value: &str) -> Result<String, String> {
    let base_name = value.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let mut cleaned = String::with_capacity(base_name.len());
    for character in base_name.chars() {
        if character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            cleaned.push('_');
        } else {
            cleaned.push(character);
        }
    }
    let cleaned = cleaned.trim_matches([' ', '.']);
    if cleaned.is_empty() {
        return Err("附件文件名无效".to_string());
    }
    let extension = file_extension(cleaned)?.to_ascii_lowercase();
    if mime_type_for_extension(&extension).is_none() {
        return Err("仅支持 PDF、TXT、Markdown、DOCX、XLSX、PPTX 和 ZIP 文件".to_string());
    }
    let suffix = format!(".{extension}");
    let stem = &cleaned[..cleaned.len() - suffix.len()];
    let max_stem_chars = 255usize.saturating_sub(suffix.chars().count());
    let truncated_stem: String = stem.chars().take(max_stem_chars).collect();
    let truncated_stem = truncated_stem.trim_matches([' ', '.']);
    if truncated_stem.is_empty() {
        return Err("附件文件名无效".to_string());
    }
    let safe_stem = if is_windows_reserved_name(truncated_stem) {
        format!("_{truncated_stem}")
    } else {
        truncated_stem.to_string()
    };
    Ok(format!("{safe_stem}{suffix}"))
}

fn is_windows_reserved_name(value: &str) -> bool {
    let upper = value.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn file_extension(display_name: &str) -> Result<&str, String> {
    Path::new(display_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .ok_or_else(|| "附件缺少受支持的扩展名".to_string())
}

fn mime_type_for_extension(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "md" | "markdown" => Some("text/markdown"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Some("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        "zip" => Some("application/zip"),
        _ => None,
    }
}

fn detect_file_mime_type(display_name: &str, bytes: &[u8]) -> Result<&'static str, String> {
    let extension = file_extension(display_name)?.to_ascii_lowercase();
    let mime_type =
        mime_type_for_extension(&extension).ok_or_else(|| "不支持此附件类型".to_string())?;
    let valid = match extension.as_str() {
        "pdf" => bytes.starts_with(b"%PDF-"),
        "txt" | "md" | "markdown" => !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok(),
        "docx" | "xlsx" | "pptx" | "zip" => {
            bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04])
                || bytes.starts_with(&[0x50, 0x4b, 0x05, 0x06])
                || bytes.starts_with(&[0x50, 0x4b, 0x07, 0x08])
        }
        _ => false,
    };
    if !valid {
        return Err("附件内容与文件类型不一致或文件已损坏".to_string());
    }
    Ok(mime_type)
}

fn mime_type(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        _ => unreachable!("format is validated before MIME conversion"),
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("创建附件文件失败：{error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("写入附件文件失败：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("同步附件文件失败：{error}"))
}

fn hash_file(path: &Path, max_bytes: i64) -> Result<(i64, String), String> {
    let file = File::open(path).map_err(|error| format!("打开附件文件失败：{error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_i64;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("读取附件文件失败：{error}"))?;
        if count == 0 {
            break;
        }
        total += count as i64;
        if total > max_bytes {
            return Err("附件文件超过校验上限".to_string());
        }
        hasher.update(&buffer[..count]);
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn relative_asset_path(attachment_uuid: &str, file_name: &str) -> String {
    format!("{ASSET_DIRECTORY}/{attachment_uuid}/{file_name}")
}

fn validated_asset_relative_path(attachment_uuid: &str, file_name: &str) -> Result<String, String> {
    validate_uuid(attachment_uuid)?;
    if file_name != ORIGINAL_FILE_NAME && file_name != PREVIEW_FILE_NAME {
        return Err("附件对象类型无效".to_string());
    }
    Ok(relative_asset_path(attachment_uuid, file_name))
}

fn validate_uuid(value: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| "附件 UUID 无效".to_string())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, Rgba, RgbaImage};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("eggdone-note-assets-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn imports_transparent_png_and_builds_bounded_jpeg_preview() {
        let root = TestDirectory::new();
        let source = root.0.join("透明图片.png");
        let mut rgba = RgbaImage::from_pixel(800, 400, Rgba([255, 0, 0, 128]));
        rgba.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        DynamicImage::ImageRgba8(rgba)
            .save_with_format(&source, ImageFormat::Png)
            .unwrap();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());
        let uuid = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

        let prepared = store.import_image(&source, uuid).unwrap();

        assert_eq!(prepared.display_name, "透明图片.png");
        assert_eq!(prepared.mime_type, "image/png");
        assert_eq!((prepared.width, prepared.height), (800, 400));
        assert_eq!(
            prepared.local_original_path,
            format!("note-assets/{uuid}/original")
        );
        assert_eq!(
            prepared.local_preview_path,
            format!("note-assets/{uuid}/preview.jpg")
        );
        assert!(store
            .verify_local_file(
                &prepared.local_original_path,
                prepared.byte_size,
                &prepared.sha256,
            )
            .unwrap());
        let preview_path = app_data.join(&prepared.local_preview_path);
        let preview = image::open(preview_path).unwrap().to_rgb8();
        assert_eq!(preview.dimensions(), (512, 256));
        assert!(preview.get_pixel(0, 0)[0] > 100);
        assert!(preview.get_pixel(0, 0)[1] > 100);
        assert!(preview.get_pixel(0, 0)[2] > 100);
    }

    #[test]
    fn imports_jpeg_and_webp_fixtures_with_bounded_jpeg_previews() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());
        let fixtures = [
            (
                "sample.jpg",
                ImageFormat::Jpeg,
                "image/jpeg",
                "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            ),
            (
                "sample.webp",
                ImageFormat::WebP,
                "image/webp",
                "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            ),
        ];

        for (file_name, format, expected_mime, uuid) in fixtures {
            let source = root.0.join(file_name);
            DynamicImage::ImageRgb8(RgbImage::from_pixel(640, 320, Rgb([38, 132, 204])))
                .save_with_format(&source, format)
                .unwrap();

            let prepared = store.import_image(&source, uuid).unwrap();

            assert_eq!(prepared.display_name, file_name);
            assert_eq!(prepared.mime_type, expected_mime);
            assert_eq!((prepared.width, prepared.height), (640, 320));
            let preview_path = app_data.join(&prepared.local_preview_path);
            let preview = image::open(preview_path).unwrap();
            assert_eq!(preview.dimensions(), (512, 256));
        }
    }

    #[test]
    fn rejects_corrupt_and_oversized_images_without_leaving_asset_directories() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());
        let corrupt = root.0.join("损坏.png");
        fs::write(&corrupt, b"not an image").unwrap();
        let corrupt_uuid = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        assert!(store.import_image(&corrupt, corrupt_uuid).is_err());
        assert!(!app_data.join("note-assets").join(corrupt_uuid).exists());

        let oversized = root.0.join("过大.png");
        let file = File::create(&oversized).unwrap();
        file.set_len(IMAGE_UPLOAD_MAX_BYTES as u64 + 1).unwrap();
        let oversized_uuid = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        assert!(store.import_image(&oversized, oversized_uuid).is_err());
        assert!(!app_data.join("note-assets").join(oversized_uuid).exists());
    }

    #[test]
    fn imports_safe_files_and_cleans_names_without_overwriting_duplicates() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());
        let first_uuid = "11111111-1111-4111-8111-111111111111";
        let second_uuid = "22222222-2222-4222-8222-222222222222";

        let first = store
            .import_file_bytes("蛋定 Todo".as_bytes(), "../计划<草稿>.TXT", first_uuid)
            .unwrap();
        let second = store
            .import_file_bytes("另一份内容".as_bytes(), "计划_草稿_.txt", second_uuid)
            .unwrap();

        assert_eq!(first.display_name, "计划_草稿_.txt");
        assert_eq!(first.mime_type, "text/plain");
        assert_eq!(second.display_name, first.display_name);
        assert_ne!(first.local_original_path, second.local_original_path);
        assert!(store
            .verify_local_file(&first.local_original_path, first.byte_size, &first.sha256)
            .unwrap());
    }

    #[test]
    fn validates_supported_file_signatures_and_rejects_unsafe_types() {
        assert_eq!(
            detect_file_mime_type("说明.pdf", b"%PDF-1.7\n").unwrap(),
            "application/pdf"
        );
        assert_eq!(
            detect_file_mime_type("资料.docx", b"PK\x03\x04word/document.xml").unwrap(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert!(detect_file_mime_type("伪装.pdf", b"MZ executable").is_err());
        assert!(sanitize_file_display_name("脚本.ps1").is_err());
        assert!(sanitize_file_display_name("程序.exe").is_err());
        assert_eq!(sanitize_file_display_name("CON.txt").unwrap(), "_CON.txt");
        assert!(detect_file_mime_type("二进制.txt", b"hello\0world").is_err());
    }

    #[test]
    fn truncates_long_file_names_and_enforces_twenty_mib_limit() {
        let long_name = format!("{}.PDF", "长".repeat(300));
        let sanitized = sanitize_file_display_name(&long_name).unwrap();
        assert_eq!(sanitized.chars().count(), 255);
        assert!(sanitized.ends_with(".pdf"));

        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data);
        let bytes = vec![b'a'; FILE_UPLOAD_MAX_BYTES as usize + 1];
        assert!(store
            .import_file_bytes(&bytes, "过大.txt", "33333333-3333-4333-8333-333333333333")
            .is_err());
    }

    #[test]
    fn verifies_and_deletes_only_uuid_scoped_assets() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        let uuid = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
        let directory = app_data.join("note-assets").join(uuid);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("original"), b"eggdone").unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());

        assert!(store
            .verify_local_file("../outside", 7, &sha256_bytes(b"eggdone"))
            .is_err());
        assert!(store.delete_asset("../outside").is_err());
        store.delete_asset(uuid).unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn writes_verified_downloads_and_reuses_valid_cache() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        fs::create_dir(&app_data).unwrap();
        let store = NoteAssetStore::for_root(app_data.clone());
        let uuid = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
        let bytes = b"downloaded eggdone image";
        let sha256 = sha256_bytes(bytes);

        let relative = store
            .write_downloaded_asset(uuid, "original", bytes, bytes.len() as i64, &sha256)
            .unwrap();
        assert_eq!(relative, format!("note-assets/{uuid}/original"));
        assert_eq!(
            store
                .read_asset_file(uuid, "original", bytes.len() as i64, &sha256)
                .unwrap(),
            bytes
        );
        assert_eq!(
            store
                .write_downloaded_asset(uuid, "original", bytes, bytes.len() as i64, &sha256)
                .unwrap(),
            relative
        );
        assert!(store
            .write_downloaded_asset(uuid, "preview.png", bytes, bytes.len() as i64, &sha256)
            .is_err());
    }

    #[test]
    fn reports_and_clears_only_uploaded_attachment_cache() {
        let root = TestDirectory::new();
        let app_data = root.0.join("app-data");
        let uploaded_uuid = "11111111-1111-4111-8111-111111111111";
        let protected_uuid = "22222222-2222-4222-8222-222222222222";
        let uploaded_directory = app_data.join("note-assets").join(uploaded_uuid);
        let protected_directory = app_data.join("note-assets").join(protected_uuid);
        fs::create_dir_all(&uploaded_directory).unwrap();
        fs::create_dir_all(&protected_directory).unwrap();
        fs::write(uploaded_directory.join("original"), b"uploaded-original").unwrap();
        fs::write(uploaded_directory.join("preview.jpg"), b"uploaded-preview").unwrap();
        fs::write(protected_directory.join("original"), b"pending-original").unwrap();
        fs::write(protected_directory.join("preview.jpg"), b"pending-preview").unwrap();
        let store = NoteAssetStore::for_root(app_data);
        let attachments = vec![
            cached_attachment(uploaded_uuid, true),
            cached_attachment(protected_uuid, false),
        ];

        let stats = store.cache_stats(&attachments).unwrap();
        assert_eq!(stats.file_count, 4);
        assert_eq!(stats.reclaimable_file_count, 2);
        assert_eq!(stats.protected_file_count, 2);
        assert_eq!(
            stats.reclaimable_bytes,
            (b"uploaded-original".len() + b"uploaded-preview".len()) as u64
        );
        assert_eq!(
            stats.protected_bytes,
            (b"pending-original".len() + b"pending-preview".len()) as u64
        );

        store.clear_reclaimable_cache(&attachments).unwrap();

        assert!(!uploaded_directory.exists());
        assert!(protected_directory.join("original").is_file());
        assert!(protected_directory.join("preview.jpg").is_file());
        let remaining = store.cache_stats(&attachments).unwrap();
        assert_eq!(remaining.total_bytes, stats.protected_bytes);
        assert_eq!(remaining.reclaimable_bytes, 0);
        assert_eq!(remaining.protected_bytes, stats.protected_bytes);
    }

    fn cached_attachment(uuid: &str, remote_uploaded: bool) -> NoteAttachment {
        NoteAttachment {
            id: 1,
            uuid: uuid.to_string(),
            note_uuid: "33333333-3333-4333-8333-333333333333".to_string(),
            kind: "image".to_string(),
            display_name: "cache.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            byte_size: 1,
            sha256: "a".repeat(64),
            preview_mime_type: Some("image/jpeg".to_string()),
            preview_byte_size: Some(1),
            preview_sha256: Some("b".repeat(64)),
            width: Some(1),
            height: Some(1),
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
            updated_by: "test-device".to_string(),
            local_original_path: Some(format!("note-assets/{uuid}/original")),
            local_preview_path: Some(format!("note-assets/{uuid}/preview.jpg")),
            transfer_state: if remote_uploaded {
                "cached".to_string()
            } else {
                "pending_upload".to_string()
            },
            transfer_error: None,
            remote_uploaded,
        }
    }
}
