use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use http::{HeaderMap, HeaderName, HeaderValue};
use keyring::{Entry, Error as KeyringError};
use rusqlite::{params, Connection};
use s3::{bucket::Bucket, creds::Credentials, region::Region};
use s3::{command::Command, request::tokio_backend::ReqwestRequest, request::Request};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::{
    db::{device_id, now_millis},
    note_attachment_sync::{self, NoteAttachmentSyncDocument},
    note_attachments::{IMAGE_UPLOAD_MAX_BYTES, PREVIEW_MAX_BYTES},
    note_sync::{self, NoteSyncDocument},
    sync::{self, SyncDocument},
};

const CREDENTIAL_SERVICE: &str = "com.eggdone.desktop.s3";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub object_key: String,
    pub note_object_key: String,
    pub note_attachment_object_key: String,
    pub note_asset_prefix: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub credentials_configured: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSyncSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub object_key: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub message: String,
    pub object_exists: bool,
}

pub struct PreparedConnectionTest {
    bucket: Box<Bucket>,
    object_key: String,
}

#[derive(Clone)]
pub struct PreparedManualSync {
    target_epoch: String,
    bucket: Box<Bucket>,
    object_key: String,
    note_object_key: String,
    note_attachment_object_key: String,
    note_asset_prefix: String,
}

impl PreparedManualSync {
    pub(crate) fn is_versioned_space(&self) -> Result<bool, String> {
        Ok(crate::sync_space::scope(&self.object_key)?.is_some())
    }
    pub(crate) fn template_transport(
        &self,
    ) -> Result<crate::task_checklist_transport::TaskChecklistTransport, String> {
        crate::task_checklist_transport::TaskChecklistTransport::templates(
            &self.bucket,
            &self.object_key,
            &[
                self.note_object_key.clone(),
                self.note_attachment_object_key.clone(),
                crate::recurrence_protocol::recurrence_object_key(&self.object_key, &[])?,
                crate::task_note_link_protocol::object_key(&self.object_key, &[])?,
                crate::task_checklist_sync::Domain::Items.object_key(&self.object_key, &[])?,
                crate::task_checklist_sync::Domain::Definitions
                    .object_key(&self.object_key, &[])?,
            ],
        )
    }
    pub(crate) fn checklist_transport(
        &self,
        domain: crate::task_checklist_sync::Domain,
    ) -> Result<crate::task_checklist_transport::TaskChecklistTransport, String> {
        crate::task_checklist_transport::TaskChecklistTransport::new(
            &self.bucket,
            &self.object_key,
            &[
                self.note_object_key.clone(),
                self.note_attachment_object_key.clone(),
                crate::recurrence_protocol::recurrence_object_key(&self.object_key, &[])?,
                crate::task_note_link_protocol::object_key(&self.object_key, &[])?,
            ],
            domain,
        )
    }
    pub(crate) fn epoch(&self) -> &str {
        &self.target_epoch
    }

    pub(crate) fn link_transport(
        &self,
    ) -> Result<crate::task_note_link_transport::TaskNoteLinkTransport, String> {
        let rule_key = crate::recurrence_protocol::recurrence_object_key(&self.object_key, &[])?;
        crate::task_note_link_transport::TaskNoteLinkTransport::new(
            &self.bucket,
            &self.object_key,
            &[
                self.note_object_key.clone(),
                self.note_attachment_object_key.clone(),
                rule_key,
            ],
        )
    }
    pub(crate) fn require_current(&self, connection: &Connection) -> Result<(), String> {
        if self.target_is_current(connection)? {
            Ok(())
        } else {
            Err("RECURRENCE_CONFIG_CHANGED".into())
        }
    }
    #[cfg(test)]
    pub(crate) fn from_test_bucket(connection: &Connection, bucket: Box<Bucket>) -> Self {
        Self {
            target_epoch: crate::sync_target::capture(connection).unwrap(),
            bucket,
            object_key: "account/todos.json".into(),
            note_object_key: "account/notes.json".into(),
            note_attachment_object_key: "account/note-attachments.json".into(),
            note_asset_prefix: "account/assets/".into(),
        }
    }
    #[cfg(test)]
    pub(crate) fn from_test_space(
        connection: &Connection,
        bucket: Box<Bucket>,
        main: &str,
    ) -> Self {
        Self {
            target_epoch: crate::sync_target::capture(connection).unwrap(),
            bucket,
            object_key: main.into(),
            note_object_key: derive_note_object_key(main),
            note_attachment_object_key: derive_note_attachment_object_key(main),
            note_asset_prefix: derive_note_asset_prefix(main),
        }
    }
    pub(crate) fn target_is_current(&self, connection: &Connection) -> Result<bool, String> {
        crate::sync_target::is_current(connection, &self.target_epoch)
    }

    pub(crate) fn recurrence_transport(
        &self,
    ) -> Result<crate::recurrence_transport::RecurrenceTransport, String> {
        crate::recurrence_transport::RecurrenceTransport::new(
            &self.bucket,
            &self.object_key,
            &[
                self.note_object_key.clone(),
                self.note_attachment_object_key.clone(),
            ],
        )
    }
}

pub struct RemoteSyncObject {
    pub document: Option<SyncDocument>,
    etag: Option<String>,
}

pub struct RemoteNoteSyncObject {
    pub document: Option<NoteSyncDocument>,
    etag: Option<String>,
}

pub struct RemoteNoteAttachmentSyncObject {
    pub document: Option<NoteAttachmentSyncDocument>,
    etag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncState {
    pub template_token: String,
    pub checklist_token: String,
    pub link_token: String,
    pub recurrence_token: String,
    pub todo_object_exists: bool,
    pub todo_etag: Option<String>,
    pub note_object_exists: bool,
    pub note_etag: Option<String>,
    pub note_attachment_object_exists: bool,
    pub note_attachment_etag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSyncResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_remote_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklist_remote_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_remote_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_remote_token: Option<String>,
    pub message: String,
    pub todo_count: usize,
    pub note_count: usize,
    pub note_attachment_count: usize,
    pub pending_attachment_count: usize,
    pub conflict_retried: bool,
    pub todo_remote_etag: Option<String>,
    pub note_remote_etag: Option<String>,
    pub note_attachment_remote_etag: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UploadOutcome {
    Success,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAssetState {
    pub exists: bool,
    pub etag: Option<String>,
    pub content_length: Option<i64>,
    pub content_type: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AssetUploadOutcome {
    Uploaded,
    AlreadyPresent,
}

#[derive(Default)]
pub struct SyncRuntime {
    in_progress: AtomicBool,
    active_asset_uuids: Mutex<HashSet<String>>,
}

pub struct SyncGuard<'a> {
    runtime: &'a SyncRuntime,
}

pub struct AssetTransferGuard<'a> {
    runtime: &'a SyncRuntime,
    attachment_uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredCredentials {
    access_key: String,
    secret_key: String,
}

#[derive(Clone, Debug)]
struct StoredSyncSettings {
    enabled: bool,
    endpoint: String,
    region: String,
    bucket: String,
    object_key: String,
    path_style: bool,
    allow_http: bool,
}

pub fn get_settings(connection: &Connection) -> Result<SyncSettings, String> {
    let settings = read_settings(connection)?;
    Ok(settings.to_public(credentials_configured(connection)?))
}

pub fn save_settings(
    connection: &Connection,
    input: SaveSyncSettings,
) -> Result<SyncSettings, String> {
    let settings = StoredSyncSettings::from_input(&input)?;
    validate_credential_input(&input)?;

    crate::sync_target::invalidate(connection)?;

    if let (Some(access_key), Some(secret_key)) = (&input.access_key, &input.secret_key) {
        store_credentials(connection, access_key, secret_key)?;
    }

    connection
        .execute(
            "
            UPDATE sync_settings
            SET enabled = ?1, endpoint = ?2, region = ?3, bucket = ?4,
                object_key = ?5, path_style = ?6, allow_http = ?7, updated_at = ?8
            WHERE id = 1
            ",
            params![
                settings.enabled,
                settings.endpoint,
                settings.region,
                settings.bucket,
                settings.object_key,
                settings.path_style,
                settings.allow_http,
                now_millis(),
            ],
        )
        .map_err(|error| format!("保存同步配置失败：{error}"))?;

    crate::sync_target::activate(connection)?;
    get_settings(connection)
}

pub fn delete_credentials(connection: &Connection) -> Result<(), String> {
    crate::sync_target::invalidate(connection)?;
    let entry = credential_entry(connection)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {
            connection
                .execute(
                    "UPDATE sync_settings SET enabled = 0, updated_at = ?1 WHERE id = 1",
                    params![now_millis()],
                )
                .map_err(|error| format!("禁用同步失败：{error}"))?;
            crate::sync_target::activate(connection)?;
            Ok(())
        }
        Err(error) => Err(format!("删除系统凭据失败：{error}")),
    }
}

pub fn prepare_connection_test(connection: &Connection) -> Result<PreparedConnectionTest, String> {
    let settings = read_settings(connection)?;
    settings.validate_connection()?;
    let credentials = load_credentials(connection)?
        .ok_or_else(|| "请先填写并保存 Access Key 和 Secret Key".to_string())?;
    let bucket = build_bucket(&settings, credentials)?;

    Ok(PreparedConnectionTest {
        bucket,
        object_key: settings.object_key,
    })
}

pub async fn test_connection(
    prepared: PreparedConnectionTest,
) -> Result<ConnectionTestResult, String> {
    let (_, status) = prepared
        .bucket
        .head_object(&prepared.object_key)
        .await
        .map_err(|error| format!("连接 S3 服务失败：{error}"))?;

    match status {
        200..=299 => Ok(ConnectionTestResult {
            message: "连接成功，已找到同步文件".to_string(),
            object_exists: true,
        }),
        404 => Ok(ConnectionTestResult {
            message: "连接成功，同步文件尚未创建".to_string(),
            object_exists: false,
        }),
        401 | 403 => Err("连接失败：凭据无效或没有 Bucket 访问权限".to_string()),
        _ => Err(format!("连接失败，S3 服务返回状态码 {status}")),
    }
}

impl SyncRuntime {
    pub fn acquire(&self) -> Result<SyncGuard<'_>, String> {
        self.in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| SyncGuard { runtime: self })
            .map_err(|_| "同步正在进行，请稍候".to_string())
    }

    pub fn acquire_asset(&self, attachment_uuid: &str) -> Result<AssetTransferGuard<'_>, String> {
        validate_asset_uuid(attachment_uuid)?;
        let mut active = self
            .active_asset_uuids
            .lock()
            .map_err(|_| "附件传输状态不可用，请重试".to_string())?;
        if !active.insert(attachment_uuid.to_string()) {
            return Err("该附件正在传输，请稍候".to_string());
        }
        Ok(AssetTransferGuard {
            runtime: self,
            attachment_uuid: attachment_uuid.to_string(),
        })
    }
}

impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        self.runtime.in_progress.store(false, Ordering::Release);
    }
}

impl Drop for AssetTransferGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.runtime.active_asset_uuids.lock() {
            active.remove(&self.attachment_uuid);
        }
    }
}

pub fn prepare_manual_sync(connection: &Connection) -> Result<PreparedManualSync, String> {
    prepare_manual_sync_using(connection, |settings| {
        let credentials = load_credentials(connection)?
            .ok_or_else(|| "请先填写并保存 Access Key 和 Secret Key".to_string())?;
        build_bucket(settings, credentials)
    })
}
#[cfg(test)]
pub(crate) fn prepare_with_fixture_credentials(
    connection: &Connection,
    bucket: Box<Bucket>,
) -> Result<PreparedManualSync, String> {
    prepare_manual_sync_using(connection, |_| Ok(bucket))
}
fn prepare_manual_sync_using(
    connection: &Connection,
    credentials: impl FnOnce(&StoredSyncSettings) -> Result<Box<Bucket>, String>,
) -> Result<PreparedManualSync, String> {
    let settings = read_settings(connection)?;
    crate::space_activation::admit(connection, &settings.object_key)?;
    if !settings.enabled {
        return Err("请先启用并保存同步配置".to_string());
    }
    settings.validate_connection()?;
    let bucket = credentials(&settings)?;
    Ok(PreparedManualSync {
        target_epoch: crate::sync_target::capture(connection)?,
        bucket,
        note_object_key: derive_note_object_key(&settings.object_key),
        note_attachment_object_key: derive_note_attachment_object_key(&settings.object_key),
        note_asset_prefix: derive_note_asset_prefix(&settings.object_key),
        object_key: settings.object_key,
    })
}

// Deliberately exposes no write operation and does not create an epoch or clear dirty state.
// The backup coordinator checks its complete source snapshot before and after each request.
pub struct MigrationAssetSource(PreparedManualSync);

pub struct MigrationStagingTarget {
    bucket: Box<Bucket>,
    prefix: String,
}
impl MigrationStagingTarget {
    pub fn new(source: &MigrationAssetSource, operation: &str) -> Result<Self, String> {
        Ok(Self {
            bucket: source.0.bucket.clone(),
            prefix: crate::migration_backup::publication::prefix(source.main_key(), operation)?,
        })
    }
    fn allowed(&self, key: &str) -> Result<(), String> {
        let suffix = key
            .strip_prefix(&self.prefix)
            .ok_or("MIGRATION_PUBLICATION_SCOPE")?;
        if suffix != "manifest.json"
            && !(suffix.starts_with("objects/")
                && suffix.len() == 72
                && suffix[8..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        {
            return Err("MIGRATION_PUBLICATION_SCOPE".into());
        }
        Ok(())
    }
}
impl crate::migration_backup::publication::Storage for MigrationStagingTarget {
    fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
        self.allowed(key)?;
        read_migration_object(&self.bucket, key, limit)
    }
    fn create(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        self.allowed(key)?;
        create_migration_object(&self.bucket, key, bytes, "application/octet-stream")
    }
}

fn read_migration_object(
    bucket: &Bucket,
    key: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, String> {
    if limit > 20 * 1024 * 1024 {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    tauri::async_runtime::block_on(async {
        let req = ReqwestRequest::new(bucket, key, Command::GetObject)
            .await
            .map_err(|_| "MIGRATION_PUBLICATION_NETWORK")?;
        let mut response = req
            .response()
            .await
            .map_err(|_| "MIGRATION_PUBLICATION_NETWORK")?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        if response.status().as_u16() != 200 {
            return Err("MIGRATION_PUBLICATION_NETWORK".into());
        }
        let tags = response
            .headers()
            .get_all("etag")
            .iter()
            .collect::<Vec<_>>();
        if tags.len() != 1
            || !tags[0]
                .to_str()
                .is_ok_and(crate::migration_backup::cloud::valid_etag)
        {
            return Err("MIGRATION_PUBLICATION_CONFLICT".into());
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err("MIGRATION_PUBLICATION_CONFLICT".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "MIGRATION_PUBLICATION_NETWORK")?
        {
            if chunk.len() > limit - bytes.len() {
                return Err("MIGRATION_PUBLICATION_CONFLICT".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Some(bytes))
    })
}
fn create_migration_object(
    bucket: &Bucket,
    key: &str,
    bytes: &[u8],
    mime: &str,
) -> Result<(), String> {
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    let mut bucket = bucket.clone();
    bucket.extra_headers.insert(
        http::HeaderName::from_static("if-none-match"),
        http::HeaderValue::from_static("*"),
    );
    bucket.extra_headers.insert(
        http::HeaderName::from_static("x-amz-meta-sha256"),
        http::HeaderValue::from_str(&sha256_hex(bytes)).map_err(|_| "MIGRATION_SPACE_INVALID")?,
    );
    let response =
        tauri::async_runtime::block_on(bucket.put_object_with_content_type(key, bytes, mime))
            .map_err(|_| "MIGRATION_PUBLICATION_NETWORK")?;
    if ![200, 201, 204, 409, 412].contains(&response.status_code()) {
        return Err("MIGRATION_PUBLICATION_NETWORK".into());
    }
    // A conflict is not success: the coordinator must read back the exact expected bytes.
    Ok(())
}
pub struct MigrationSpaceTarget {
    bucket: Box<Bucket>,
    main: String,
    claim: Option<crate::space_activation::Claim>,
}
impl MigrationSpaceTarget {
    pub fn new(
        source: &MigrationAssetSource,
        claim: Option<crate::space_activation::Claim>,
    ) -> Result<Self, String> {
        if claim.as_ref().is_some_and(|c| c.1 != source.main_key()) {
            return Err("MIGRATION_SPACE_CHANGED".into());
        }
        Ok(Self {
            bucket: source.0.bucket.clone(),
            main: source.main_key().into(),
            claim,
        })
    }
    fn allowed(&self, key: &str, write: bool) -> Result<(), String> {
        if key == crate::space_activation::claim_key(&self.main) {
            return Ok(());
        }
        if let Some(claim) = &self.claim {
            let p = claim.plan()?;
            if !write
                && (key == crate::migration_backup::publication::marker_key(&p)?
                    || p.files.iter().any(|f| {
                        crate::migration_backup::publication::object_key(&p, f).as_deref()
                            == Ok(key)
                    }))
            {
                return Ok(());
            }
            let main = claim.main()?;
            let prefix = main.trim_end_matches("todos.json");
            if key == claim.ready_key()?
                || crate::sync_space::scope(key)
                    .ok()
                    .flatten()
                    .is_some_and(|(id, _)| id == p.operation)
            {
                return Ok(());
            }
            if let Some(asset) = key.strip_prefix(&format!("{prefix}note-assets/v1/")) {
                if let Some((id, suffix)) = asset.split_once('/') {
                    if uuid::Uuid::parse_str(id).is_ok()
                        && ["original", "preview.jpg"].contains(&suffix)
                    {
                        return Ok(());
                    }
                }
            }
        }
        Err("MIGRATION_PUBLICATION_SCOPE".into())
    }
}
impl crate::space_activation::Storage for MigrationSpaceTarget {
    fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
        self.allowed(key, false)?;
        read_migration_object(&self.bucket, key, limit)
    }
    fn create(&mut self, key: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
        self.allowed(key, true)?;
        create_migration_object(&self.bucket, key, bytes, mime)
    }
}

pub fn migration_source_binding(connection: &Connection) -> Result<String, String> {
    let s = read_settings(connection).map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?;
    s.validate_connection()
        .map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?;
    let bytes = serde_json::to_vec(&serde_json::json!([
        s.endpoint,
        s.region,
        s.bucket,
        s.object_key,
        s.path_style,
        s.allow_http
    ]))
    .map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?;
    Ok(sha256_hex(&bytes))
}

pub fn prepare_migration_asset_source(
    connection: &Connection,
) -> Result<MigrationAssetSource, String> {
    let settings = read_settings(connection).map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?;
    settings
        .validate_connection()
        .map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?;
    let credentials = load_credentials(connection)
        .map_err(|_| "MIGRATION_BACKUP_ASSET_CREDENTIALS")?
        .ok_or("MIGRATION_BACKUP_ASSET_CREDENTIALS")?;
    Ok(MigrationAssetSource(PreparedManualSync {
        target_epoch: String::new(),
        bucket: build_bucket(&settings, credentials)
            .map_err(|_| "MIGRATION_BACKUP_ASSET_CONFIG")?,
        note_object_key: derive_note_object_key(&settings.object_key),
        note_attachment_object_key: derive_note_attachment_object_key(&settings.object_key),
        note_asset_prefix: derive_note_asset_prefix(&settings.object_key),
        object_key: settings.object_key,
    }))
}

impl MigrationAssetSource {
    #[cfg(test)]
    pub(crate) fn from_test_bucket(bucket: Box<Bucket>, main: &str) -> Self {
        Self(PreparedManualSync {
            target_epoch: String::new(),
            bucket,
            object_key: main.into(),
            note_object_key: derive_note_object_key(main),
            note_attachment_object_key: derive_note_attachment_object_key(main),
            note_asset_prefix: derive_note_asset_prefix(main),
        })
    }
    pub fn main_key(&self) -> &str {
        &self.0.object_key
    }

    pub async fn metadata(
        &self,
    ) -> Result<Vec<crate::migration_backup::cloud::RemoteObject>, String> {
        use crate::migration_backup::cloud::{
            object_keys, valid_etag, validate_document, RemoteObject, MAX_OBJECT,
        };
        let mut result = Vec::new();
        let mut total = 0;
        for (index, key) in object_keys(self.main_key())?.iter().enumerate() {
            let request = ReqwestRequest::new(&self.0.bucket, key, Command::GetObject)
                .await
                .map_err(|_| "MIGRATION_CLOUD_DOWNLOAD")?;
            let mut response = request
                .response()
                .await
                .map_err(|_| "MIGRATION_CLOUD_DOWNLOAD")?;
            if response.status().as_u16() == 404 {
                result.push(RemoteObject {
                    etag: None,
                    bytes: None,
                });
                continue;
            }
            if response.status().as_u16() != 200 {
                return Err("MIGRATION_CLOUD_DOWNLOAD".into());
            }
            let values = response
                .headers()
                .get_all("etag")
                .iter()
                .collect::<Vec<_>>();
            if values.len() != 1 {
                return Err("MIGRATION_CLOUD_INVALID".into());
            }
            let etag = values[0]
                .to_str()
                .map_err(|_| "MIGRATION_CLOUD_INVALID")?
                .to_string();
            if !valid_etag(&etag) {
                return Err("MIGRATION_CLOUD_INVALID".into());
            }
            if response
                .content_length()
                .is_some_and(|n| n > (MAX_OBJECT - total) as u64)
            {
                return Err("MIGRATION_BACKUP_LIMIT".into());
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "MIGRATION_CLOUD_DOWNLOAD")?
            {
                if chunk.len() > MAX_OBJECT - total {
                    return Err("MIGRATION_BACKUP_LIMIT".into());
                }
                total += chunk.len();
                bytes.extend_from_slice(&chunk);
            }
            validate_document(index, &bytes)?;
            result.push(RemoteObject {
                etag: Some(etag),
                bytes: Some(bytes),
            });
        }
        Ok(result)
    }

    pub async fn download(
        &self,
        runtime: &SyncRuntime,
        uuid: &str,
        name: &str,
        size: i64,
        sha256: &str,
    ) -> Result<Vec<u8>, String> {
        download_asset_bytes(runtime, &self.0, uuid, name, size, sha256)
            .await
            .map_err(|error| {
                let code = migration_asset_error(&error);
                if uuid::Uuid::parse_str(uuid).is_ok()
                    && ["original", "preview.jpg"].contains(&name)
                {
                    format!("{code}:{uuid}:{name}")
                } else {
                    code.into()
                }
            })
    }
}

// Convert the existing download API's errors to diagnostics without paths, URLs or credentials.
fn migration_asset_error(error: &str) -> &'static str {
    if error.contains("远端文件不存在") {
        "MIGRATION_BACKUP_ASSET_NOT_FOUND"
    } else if error.contains("凭据无效或没有对象读取权限") {
        "MIGRATION_BACKUP_ASSET_DENIED"
    } else if error.contains("SHA-256") || error.contains("文件大小") || error.contains("预期大小")
    {
        "MIGRATION_BACKUP_ASSET_INVALID"
    } else {
        "MIGRATION_BACKUP_ASSET_DOWNLOAD"
    }
}

pub(crate) async fn download_space_json(
    bucket: &Bucket,
    key: &str,
) -> Result<(String, String), String> {
    download_json(bucket, key, false).await
}
async fn download_json(
    bucket: &Bucket,
    key: &str,
    allow_missing: bool,
) -> Result<(String, String), String> {
    let request = ReqwestRequest::new(bucket, key, Command::GetObject)
        .await
        .map_err(|_| "SYNC_SPACE_NETWORK")?;
    let mut response = request.response().await.map_err(|_| "SYNC_SPACE_NETWORK")?;
    if response.status().as_u16() == 404 {
        if allow_missing {
            return Ok((String::new(), String::new()));
        }
        return Err("SYNC_SPACE_INCOMPLETE".into());
    }
    if response.status().as_u16() != 200 {
        return Err("SYNC_SPACE_RESPONSE".into());
    }
    let mut tags = response.headers().get_all("etag").iter();
    let etag = tags
        .next()
        .and_then(|v| v.to_str().ok())
        .filter(|v| crate::migration_backup::cloud::valid_etag(v))
        .ok_or("SYNC_SPACE_ETAG")?
        .to_string();
    if tags.next().is_some() {
        return Err("SYNC_SPACE_ETAG".into());
    }
    // Match the native mobile base-domain response cap before buffering the envelope.
    let limit = if crate::sync_space::scope(key)?.is_some_and(|(_, domain)| domain == "terminals") {
        4
    } else {
        5
    } * 1024
        * 1024;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("SYNC_SPACE_LIMIT".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "SYNC_SPACE_NETWORK")? {
        if chunk.len() > limit - bytes.len() {
            return Err("SYNC_SPACE_LIMIT".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = crate::sync_space::decode(
        key,
        std::str::from_utf8(&bytes).map_err(|_| "SYNC_SPACE_UTF8")?,
    )?;
    Ok((text, etag))
}

pub async fn download_remote(prepared: &PreparedManualSync) -> Result<RemoteSyncObject, String> {
    if crate::sync_space::scope(&prepared.object_key)?.is_some() {
        let (text, etag) = download_space_json(&prepared.bucket, &prepared.object_key).await?;
        let document = serde_json::from_str(&text).map_err(|_| "SYNC_SPACE_INVALID")?;
        sync::validate_document(&document)?;
        return Ok(RemoteSyncObject {
            document: Some(document),
            etag: Some(etag),
        });
    }
    let response = prepared
        .bucket
        .get_object(&prepared.object_key)
        .await
        .map_err(|error| format!("下载同步文件失败：{error}"))?;

    match response.status_code() {
        200..=299 => {
            let etag = response
                .headers()
                .into_iter()
                .find_map(|(name, value)| name.eq_ignore_ascii_case("etag").then_some(value))
                .ok_or_else(|| "远端同步文件缺少 ETag，无法安全写入".to_string())?;
            let document = serde_json::from_slice::<SyncDocument>(response.as_slice())
                .map_err(|error| format!("远端同步文件格式无效：{error}"))?;
            sync::validate_document(&document)?;
            Ok(RemoteSyncObject {
                document: Some(document),
                etag: Some(etag),
            })
        }
        404 => {
            crate::sync_space::require_existing(&prepared.object_key, None)?;
            Ok(RemoteSyncObject {
                document: None,
                etag: None,
            })
        }
        401 | 403 => Err("下载失败：凭据无效或没有对象读取权限".to_string()),
        status => Err(format!("下载同步文件失败，S3 服务返回状态码 {status}")),
    }
}

async fn get_object_state(
    prepared: &PreparedManualSync,
    object_key: &str,
) -> Result<(bool, Option<String>), String> {
    let (headers, status) = prepared
        .bucket
        .head_object(object_key)
        .await
        .map_err(|error| format!("检查远端同步文件失败：{error}"))?;

    crate::sync_space::require_probe(object_key, status)?;
    match status {
        200..=299 => {
            let etag = headers
                .e_tag
                .ok_or_else(|| "远端同步文件缺少 ETag".to_string())?;
            if crate::sync_space::scope(object_key)?.is_some()
                && !crate::migration_backup::cloud::valid_etag(&etag)
            {
                return Err("SYNC_SPACE_ETAG".into());
            }
            Ok((true, Some(etag)))
        }
        404 => Ok((false, None)),
        401 | 403 => Err("连接失败：凭据无效或没有 Bucket 访问权限".to_string()),
        _ => Err(format!("检查远端同步文件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn get_remote_state(
    prepared: &PreparedManualSync,
    database: &crate::db::Database,
) -> Result<RemoteSyncState, String> {
    let guard = || {
        let connection = database
            .connection
            .lock()
            .map_err(|_| "RECURRENCE_DATABASE_LOCK")?;
        prepared.require_current(&connection)
    };
    guard()?;
    let result = async {
        let (todo_object_exists, todo_etag) =
            get_object_state(prepared, &prepared.object_key).await?;
        guard()?;
        let (note_object_exists, note_etag) =
            get_object_state(prepared, &prepared.note_object_key).await?;
        guard()?;
        let (note_attachment_object_exists, note_attachment_etag) =
            get_object_state(prepared, &prepared.note_attachment_object_key).await?;
        guard()?;
        let recurrence_token = prepared.recurrence_transport()?.probe().await?;
        guard()?;
        let link_token = prepared.link_transport()?.probe().await?;
        guard()?;
        let definitions = prepared
            .checklist_transport(crate::task_checklist_sync::Domain::Definitions)?
            .probe()
            .await?;
        guard()?;
        let items = prepared
            .checklist_transport(crate::task_checklist_sync::Domain::Items)?
            .probe()
            .await?;
        guard()?;
        let template_token = prepared.template_transport()?.probe().await?;
        guard()?;
        Ok(RemoteSyncState {
            template_token,
            checklist_token: serde_json::to_string(&[definitions, items])
                .map_err(|_| "CHECKLIST_TOKEN_INVALID")?,
            link_token,
            recurrence_token,
            todo_object_exists,
            todo_etag,
            note_object_exists,
            note_etag,
            note_attachment_object_exists,
            note_attachment_etag,
        })
    }
    .await;
    guard()?;
    result
}

pub async fn download_note_attachment_remote(
    prepared: &PreparedManualSync,
) -> Result<RemoteNoteAttachmentSyncObject, String> {
    if crate::sync_space::scope(&prepared.note_attachment_object_key)?.is_some() {
        let (text, etag) =
            download_space_json(&prepared.bucket, &prepared.note_attachment_object_key).await?;
        let document = serde_json::from_str(&text).map_err(|_| "SYNC_SPACE_INVALID")?;
        note_attachment_sync::validate_document(&document)?;
        return Ok(RemoteNoteAttachmentSyncObject {
            document: Some(document),
            etag: Some(etag),
        });
    }
    let response = prepared
        .bucket
        .get_object(&prepared.note_attachment_object_key)
        .await
        .map_err(|error| format!("下载附件元数据失败：{error}"))?;

    match response.status_code() {
        200..=299 => {
            let etag = response
                .headers()
                .into_iter()
                .find_map(|(name, value)| name.eq_ignore_ascii_case("etag").then_some(value))
                .ok_or_else(|| "远端附件元数据缺少 ETag，无法安全写入".to_string())?;
            let document =
                serde_json::from_slice::<NoteAttachmentSyncDocument>(response.as_slice())
                    .map_err(|error| format!("远端附件元数据格式无效：{error}"))?;
            note_attachment_sync::validate_document(&document)?;
            Ok(RemoteNoteAttachmentSyncObject {
                document: Some(document),
                etag: Some(etag),
            })
        }
        404 => {
            crate::sync_space::require_existing(&prepared.note_attachment_object_key, None)?;
            Ok(RemoteNoteAttachmentSyncObject {
                document: None,
                etag: None,
            })
        }
        401 | 403 => Err("下载附件元数据失败：凭据无效或没有对象读取权限".to_string()),
        status => Err(format!("下载附件元数据失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn download_note_remote(
    prepared: &PreparedManualSync,
) -> Result<RemoteNoteSyncObject, String> {
    if crate::sync_space::scope(&prepared.note_object_key)?.is_some() {
        let (text, etag) = download_space_json(&prepared.bucket, &prepared.note_object_key).await?;
        let document = serde_json::from_str(&text).map_err(|_| "SYNC_SPACE_INVALID")?;
        note_sync::validate_document(&document)?;
        return Ok(RemoteNoteSyncObject {
            document: Some(document),
            etag: Some(etag),
        });
    }
    let response = prepared
        .bucket
        .get_object(&prepared.note_object_key)
        .await
        .map_err(|error| format!("下载便签同步文件失败：{error}"))?;

    match response.status_code() {
        200..=299 => {
            let etag = response
                .headers()
                .into_iter()
                .find_map(|(name, value)| name.eq_ignore_ascii_case("etag").then_some(value))
                .ok_or_else(|| "远端便签同步文件缺少 ETag，无法安全写入".to_string())?;
            let document = serde_json::from_slice::<NoteSyncDocument>(response.as_slice())
                .map_err(|error| format!("远端便签同步文件格式无效：{error}"))?;
            note_sync::validate_document(&document)?;
            Ok(RemoteNoteSyncObject {
                document: Some(document),
                etag: Some(etag),
            })
        }
        404 => {
            crate::sync_space::require_existing(&prepared.note_object_key, None)?;
            Ok(RemoteNoteSyncObject {
                document: None,
                etag: None,
            })
        }
        401 | 403 => Err("下载便签失败：凭据无效或没有对象读取权限".to_string()),
        status => Err(format!("下载便签同步文件失败，S3 服务返回状态码 {status}")),
    }
}

pub(crate) fn lifecycle_key(prepared: &PreparedManualSync) -> Result<String, String> {
    let Some((id, domain)) = crate::sync_space::scope(&prepared.object_key)? else {
        use sha2::{Digest, Sha256};
        return Ok(format!(
            "eggdone-lifecycle/v1/{:x}/terminals.json",
            Sha256::digest(prepared.object_key.as_bytes())
        ));
    };
    if domain != "todos" {
        return Err("SYNC_SPACE_KEY".into());
    }
    Ok(format!(
        "{}{id}/lifecycle-terminals.json",
        crate::sync_space::PREFIX
    ))
}
pub(crate) async fn download_lifecycle(
    prepared: &PreparedManualSync,
) -> Result<(crate::lifecycle_sync::Document, String), String> {
    let (raw, etag) = download_json(
        &prepared.bucket,
        &lifecycle_key(prepared)?,
        !prepared.is_versioned_space()?,
    )
    .await?;
    if etag.is_empty() {
        return Ok((
            crate::lifecycle_sync::Document {
                format_version: 1,
                terminals: vec![],
            },
            etag,
        ));
    }
    Ok((crate::lifecycle_sync::parse(&raw)?, etag))
}
pub(crate) async fn upload_lifecycle(
    prepared: &PreparedManualSync,
    document: &crate::lifecycle_sync::Document,
    etag: &str,
) -> Result<UploadOutcome, String> {
    if !(etag.is_empty() && !prepared.is_versioned_space()?)
        && !crate::migration_backup::cloud::valid_etag(etag)
    {
        return Err("PURGE_LEDGER_ACK".into());
    }
    let key = lifecycle_key(prepared)?;
    let content = crate::sync_space::encode(&key, &crate::lifecycle_sync::encode(document)?)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(if etag.is_empty() {
            "if-none-match"
        } else {
            "if-match"
        }),
        HeaderValue::from_str(if etag.is_empty() { "*" } else { etag })
            .map_err(|_| "PURGE_LEDGER_ACK")?,
    );
    let response = prepared
        .bucket
        .put_object_builder(&key, content.as_bytes())
        .with_content_type("application/json")
        .with_headers(headers)
        .execute()
        .await
        .map_err(|_| "PURGE_LEDGER_NETWORK")?;
    classify_upload_status(response.status_code())
}

pub async fn upload_document(
    prepared: &PreparedManualSync,
    document: &SyncDocument,
    remote: &RemoteSyncObject,
) -> Result<UploadOutcome, String> {
    let content = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("生成同步文件失败：{error}"))?;
    crate::sync_space::require_existing(&prepared.object_key, remote.etag.as_deref())?;
    let content = crate::sync_space::encode(
        &prepared.object_key,
        std::str::from_utf8(&content).map_err(|_| "SYNC_SPACE_UTF8")?,
    )?
    .into_bytes();
    let (header_name, header_value) = upload_condition(remote.etag.as_deref());
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(header_name),
        HeaderValue::from_str(header_value)
            .map_err(|error| format!("创建同步条件请求失败：{error}"))?,
    );
    let response = prepared
        .bucket
        .put_object_builder(&prepared.object_key, &content)
        .with_content_type("application/json")
        .with_headers(headers)
        .execute()
        .await
        .map_err(|error| format!("上传同步文件失败：{error}"))?;

    classify_upload_status(response.status_code())
}

pub async fn upload_note_document(
    prepared: &PreparedManualSync,
    document: &NoteSyncDocument,
    remote: &RemoteNoteSyncObject,
) -> Result<UploadOutcome, String> {
    let content = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("生成便签同步文件失败：{error}"))?;
    crate::sync_space::require_existing(&prepared.note_object_key, remote.etag.as_deref())?;
    let content = crate::sync_space::encode(
        &prepared.note_object_key,
        std::str::from_utf8(&content).map_err(|_| "SYNC_SPACE_UTF8")?,
    )?
    .into_bytes();
    let (header_name, header_value) = upload_condition(remote.etag.as_deref());
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(header_name),
        HeaderValue::from_str(header_value)
            .map_err(|error| format!("创建便签同步条件请求失败：{error}"))?,
    );
    let response = prepared
        .bucket
        .put_object_builder(&prepared.note_object_key, &content)
        .with_content_type("application/json")
        .with_headers(headers)
        .execute()
        .await
        .map_err(|error| format!("上传便签同步文件失败：{error}"))?;

    classify_upload_status(response.status_code())
}

pub async fn upload_note_attachment_document(
    prepared: &PreparedManualSync,
    document: &NoteAttachmentSyncDocument,
    remote: &RemoteNoteAttachmentSyncObject,
) -> Result<UploadOutcome, String> {
    note_attachment_sync::validate_document(document)?;
    let content = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("生成附件元数据失败：{error}"))?;
    crate::sync_space::require_existing(
        &prepared.note_attachment_object_key,
        remote.etag.as_deref(),
    )?;
    let content = crate::sync_space::encode(
        &prepared.note_attachment_object_key,
        std::str::from_utf8(&content).map_err(|_| "SYNC_SPACE_UTF8")?,
    )?
    .into_bytes();
    let (header_name, header_value) = upload_condition(remote.etag.as_deref());
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(header_name),
        HeaderValue::from_str(header_value)
            .map_err(|error| format!("创建附件元数据条件请求失败：{error}"))?,
    );
    let response = prepared
        .bucket
        .put_object_builder(&prepared.note_attachment_object_key, &content)
        .with_content_type("application/json")
        .with_headers(headers)
        .execute()
        .await
        .map_err(|error| format!("上传附件元数据失败：{error}"))?;

    classify_upload_status(response.status_code())
}

pub async fn head_asset_object(
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
) -> Result<RemoteAssetState, String> {
    let object_key = asset_object_key(prepared, attachment_uuid, file_name)?;
    let (headers, status) = prepared
        .bucket
        .head_object(&object_key)
        .await
        .map_err(|error| format!("检查远端附件失败，请检查网络后重试：{error}"))?;
    match status {
        200 => Ok(RemoteAssetState {
            exists: true,
            etag: headers.e_tag,
            content_length: headers.content_length,
            content_type: headers.content_type,
            sha256: metadata_value(headers.metadata.as_ref(), "sha256"),
        }),
        404 => Ok(RemoteAssetState {
            exists: false,
            etag: None,
            content_length: None,
            content_type: None,
            sha256: None,
        }),
        401 | 403 => Err("检查远端附件失败：凭据无效或没有对象读取权限".to_string()),
        _ => Err(format!("检查远端附件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn upload_immutable_asset(
    runtime: &SyncRuntime,
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
    content: &[u8],
    content_type: &str,
    expected_sha256: &str,
) -> Result<AssetUploadOutcome, String> {
    validate_asset_payload(file_name, content, content_type, expected_sha256)?;
    let _guard = runtime.acquire_asset(attachment_uuid)?;
    let object_key = asset_object_key(prepared, attachment_uuid, file_name)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("if-none-match"),
        HeaderValue::from_static("*"),
    );
    let response = prepared
        .bucket
        .put_object_builder(&object_key, content)
        .with_content_type(content_type)
        .with_headers(headers)
        .with_metadata("sha256", expected_sha256)
        .map_err(|error| format!("创建附件上传请求失败：{error}"))?
        .execute()
        .await
        .map_err(|error| format!("上传附件失败，请检查网络后重试：{error}"))?;
    match response.status_code() {
        200..=299 => Ok(AssetUploadOutcome::Uploaded),
        409 | 412 => {
            let remote = head_asset_object(prepared, attachment_uuid, file_name).await?;
            if asset_matches(&remote, content.len() as i64, content_type, expected_sha256) {
                Ok(AssetUploadOutcome::AlreadyPresent)
            } else {
                Err("远端已有同名附件但内容不同，请重新同步元数据后重试".to_string())
            }
        }
        401 | 403 => Err("上传附件失败：凭据无效或没有对象写入权限".to_string()),
        status => Err(format!("上传附件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn download_asset_bytes(
    runtime: &SyncRuntime,
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
    expected_size: i64,
    expected_sha256: &str,
) -> Result<Vec<u8>, String> {
    validate_asset_identity(file_name, expected_size, expected_sha256)?;
    let _guard = runtime.acquire_asset(attachment_uuid)?;
    let object_key = asset_object_key(prepared, attachment_uuid, file_name)?;
    read_asset_bytes(
        &prepared.bucket,
        &object_key,
        expected_size,
        expected_sha256,
        None,
    )
    .await
}

async fn read_asset_bytes(
    bucket: &Bucket,
    object_key: &str,
    expected_size: i64,
    expected_sha256: &str,
    etag: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut bucket = bucket.clone();
    if let Some(tag) = etag {
        bucket.extra_headers.insert(
            HeaderName::from_static("if-match"),
            HeaderValue::from_str(tag).map_err(|_| "PURGE_ASSET_ETAG")?,
        );
    }
    let request = ReqwestRequest::new(&bucket, object_key, Command::GetObject)
        .await
        .map_err(|_| "下载附件失败：无法创建读取请求")?;
    let mut response = request
        .response()
        .await
        .map_err(|error| format!("下载附件失败，请检查网络后重试：{error}"))?;
    match response.status().as_u16() {
        200 => {
            if etag.is_some()
                && response.headers().get("etag").and_then(|v| v.to_str().ok()) != etag
            {
                return Err("PURGE_ASSET_CHANGED".into());
            }
            if response
                .content_length()
                .is_some_and(|n| n != expected_size as u64)
            {
                return Err("下载附件失败：文件大小与同步元数据不一致".into());
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "下载附件失败：读取中断")?
            {
                if chunk.len() > expected_size as usize - bytes.len() {
                    return Err("下载附件失败：文件超过预期大小".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.len() as i64 != expected_size {
                return Err("下载附件失败：文件大小与同步元数据不一致".to_string());
            }
            if sha256_hex(&bytes) != expected_sha256 {
                return Err("下载附件失败：SHA-256 校验不通过，请重试".to_string());
            }
            Ok(bytes)
        }
        404 => Err("下载附件失败：远端文件不存在，可稍后重试同步".to_string()),
        401 | 403 => Err("下载附件失败：凭据无效或没有对象读取权限".to_string()),
        status => Err(format!("下载附件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn delete_asset_if_matches(
    runtime: &SyncRuntime,
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
    expected_size: i64,
    expected_sha256: &str,
) -> Result<bool, String> {
    delete_asset_if_matches_guarded(
        runtime,
        prepared,
        attachment_uuid,
        file_name,
        expected_size,
        expected_sha256,
        &|| Ok(()),
    )
    .await
}
pub(crate) async fn delete_asset_if_matches_guarded(
    runtime: &SyncRuntime,
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
    expected_size: i64,
    expected_sha256: &str,
    guard: &(impl Fn() -> Result<(), String> + Sync),
) -> Result<bool, String> {
    guard()?;
    validate_asset_identity(file_name, expected_size, expected_sha256)?;
    let _guard = runtime.acquire_asset(attachment_uuid)?;
    let remote = head_asset_object(prepared, attachment_uuid, file_name).await?;
    guard()?;
    if !remote.exists {
        return Ok(false);
    }
    if remote.content_length != Some(expected_size)
        || remote
            .sha256
            .as_deref()
            .is_some_and(|sha| sha != expected_sha256)
    {
        return Err("拒绝删除远端附件：对象大小或 SHA-256 与元数据不一致".to_string());
    }
    let etag = remote
        .etag
        .as_deref()
        .filter(|value| {
            let bytes = value.as_bytes();
            (3..=1024).contains(&bytes.len())
                && bytes[0] == b'"'
                && bytes[bytes.len() - 1] == b'"'
                && bytes[1..bytes.len() - 1]
                    .iter()
                    .all(|b| *b == 0x21 || (0x23..=0x7e).contains(b))
        })
        .ok_or("拒绝删除远端附件：缺少有效 ETag，请保留文件并重试")?;
    let mut bucket = prepared.bucket.clone();
    bucket.extra_headers.insert(
        HeaderName::from_static("if-match"),
        HeaderValue::from_str(etag).map_err(|_| "拒绝删除远端附件：ETag 无效")?,
    );
    let object_key = asset_object_key(prepared, attachment_uuid, file_name)?;
    if remote.sha256.is_none() {
        // Historical objects may lack custom hash metadata. Verify only this selected file, bound to HEAD's ETag.
        read_asset_bytes(
            &prepared.bucket,
            &object_key,
            expected_size,
            expected_sha256,
            Some(etag),
        )
        .await?;
    }
    guard()?;
    let response = bucket
        .delete_object(&object_key)
        .await
        .map_err(|error| format!("删除远端附件失败，请检查网络后重试：{error}"))?;
    guard()?;
    match response.status_code() {
        200 | 204 | 404 => Ok(true),
        409 | 412 => Err("远端附件已变化，未删除；请重新同步后重试".into()),
        401 | 403 => Err("删除远端附件失败：凭据无效或没有对象删除权限".to_string()),
        status => Err(format!("删除远端附件失败，S3 服务返回状态码 {status}")),
    }
}

fn asset_object_key(
    prepared: &PreparedManualSync,
    attachment_uuid: &str,
    file_name: &str,
) -> Result<String, String> {
    validate_asset_uuid(attachment_uuid)?;
    validate_asset_file_name(file_name)?;
    Ok(format!(
        "{}{attachment_uuid}/{file_name}",
        prepared.note_asset_prefix
    ))
}

fn validate_asset_payload(
    file_name: &str,
    content: &[u8],
    content_type: &str,
    expected_sha256: &str,
) -> Result<(), String> {
    validate_asset_identity(file_name, content.len() as i64, expected_sha256)?;
    if content_type.is_empty() || content_type.len() > 100 || content_type.contains(['\r', '\n']) {
        return Err("附件 Content-Type 无效".to_string());
    }
    if sha256_hex(content) != expected_sha256 {
        return Err("附件内容与 SHA-256 元数据不一致".to_string());
    }
    Ok(())
}

fn validate_asset_identity(
    file_name: &str,
    expected_size: i64,
    expected_sha256: &str,
) -> Result<(), String> {
    validate_asset_file_name(file_name)?;
    let maximum = if file_name == "preview.jpg" {
        PREVIEW_MAX_BYTES
    } else {
        IMAGE_UPLOAD_MAX_BYTES
    };
    if expected_size <= 0 || expected_size > maximum {
        return Err("附件大小无效".to_string());
    }
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("附件 SHA-256 无效".to_string());
    }
    Ok(())
}

fn validate_asset_uuid(value: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| "附件 UUID 无效".to_string())
}

fn validate_asset_file_name(value: &str) -> Result<(), String> {
    match value {
        "original" | "preview.jpg" => Ok(()),
        _ => Err("附件对象类型无效".to_string()),
    }
}

fn metadata_value(
    metadata: Option<&std::collections::HashMap<String, String>>,
    name: &str,
) -> Option<String> {
    metadata.and_then(|values| {
        values
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then(|| value.to_lowercase()))
    })
}

fn asset_matches(
    remote: &RemoteAssetState,
    expected_size: i64,
    expected_content_type: &str,
    expected_sha256: &str,
) -> bool {
    asset_matches_without_type(remote, expected_size, expected_sha256)
        && remote
            .content_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(expected_content_type))
}

fn asset_matches_without_type(
    remote: &RemoteAssetState,
    expected_size: i64,
    expected_sha256: &str,
) -> bool {
    remote.exists
        && remote.content_length == Some(expected_size)
        && remote.sha256.as_deref() == Some(expected_sha256)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn derive_note_object_key(object_key: &str) -> String {
    if object_key == "todos.json" {
        return "notes.json".to_string();
    }
    if let Some(prefix) = object_key.strip_suffix("/todos.json") {
        return format!("{prefix}/notes.json");
    }
    let slash_index = object_key.rfind('/');
    if let Some(extension_index) = object_key.rfind('.') {
        if slash_index.is_none_or(|index| extension_index > index) {
            return format!(
                "{}.notes{}",
                &object_key[..extension_index],
                &object_key[extension_index..]
            );
        }
    }
    format!("{object_key}.notes.json")
}

pub(crate) fn derive_note_attachment_object_key(object_key: &str) -> String {
    if object_key == "todos.json" {
        return "note-attachments.json".to_string();
    }
    if let Some(prefix) = object_key.strip_suffix("/todos.json") {
        return format!("{prefix}/note-attachments.json");
    }
    let slash_index = object_key.rfind('/');
    if let Some(extension_index) = object_key.rfind('.') {
        if slash_index.is_none_or(|index| extension_index > index) {
            return format!(
                "{}.note-attachments{}",
                &object_key[..extension_index],
                &object_key[extension_index..]
            );
        }
    }
    format!("{object_key}.note-attachments.json")
}

pub(crate) fn derive_note_asset_prefix(object_key: &str) -> String {
    if object_key == "todos.json" {
        return "note-assets/v1/".to_string();
    }
    if let Some(prefix) = object_key.strip_suffix("/todos.json") {
        return format!("{prefix}/note-assets/v1/");
    }
    let slash_index = object_key.rfind('/');
    if let Some(extension_index) = object_key.rfind('.') {
        if slash_index.is_none_or(|index| extension_index > index) {
            return format!("{}.note-assets/v1/", &object_key[..extension_index]);
        }
    }
    format!("{object_key}.note-assets/v1/")
}

fn upload_condition(etag: Option<&str>) -> (&'static str, &str) {
    match etag {
        Some(value) => ("if-match", value),
        None => ("if-none-match", "*"),
    }
}

fn classify_upload_status(status: u16) -> Result<UploadOutcome, String> {
    match status {
        200..=299 => Ok(UploadOutcome::Success),
        409 | 412 => Ok(UploadOutcome::Conflict),
        401 | 403 => Err("上传失败：凭据无效或没有对象写入权限".to_string()),
        _ => Err(format!("上传同步文件失败，S3 服务返回状态码 {status}")),
    }
}

impl StoredSyncSettings {
    fn from_input(input: &SaveSyncSettings) -> Result<Self, String> {
        let settings = Self {
            enabled: input.enabled,
            endpoint: input.endpoint.trim().trim_end_matches('/').to_string(),
            region: input.region.trim().to_string(),
            bucket: input.bucket.trim().to_string(),
            object_key: input.object_key.trim().trim_start_matches('/').to_string(),
            path_style: input.path_style,
            allow_http: input.allow_http,
        };
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), String> {
        if self.enabled {
            self.validate_connection()?;
        } else {
            self.validate_endpoint()?;
        }
        Ok(())
    }

    fn validate_connection(&self) -> Result<(), String> {
        if self.region.is_empty() {
            return Err("Region 不能为空".to_string());
        }
        if self.bucket.is_empty() {
            return Err("Bucket 不能为空".to_string());
        }
        if self.object_key.is_empty() {
            return Err("Object Key 不能为空".to_string());
        }
        self.validate_endpoint()
    }

    fn validate_endpoint(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Ok(());
        }
        let endpoint =
            Url::parse(&self.endpoint).map_err(|_| "Endpoint 不是有效的网址".to_string())?;
        if endpoint.scheme() != "http" && endpoint.scheme() != "https" {
            return Err("Endpoint 只支持 HTTP 或 HTTPS".to_string());
        }
        if endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err("Endpoint 不能包含凭据、查询参数或片段".to_string());
        }
        if endpoint.scheme() == "http" && !self.allow_http {
            return Err("HTTP 会明文传输凭据和数据，请先确认风险".to_string());
        }
        Ok(())
    }

    fn to_public(&self, credentials_configured: bool) -> SyncSettings {
        SyncSettings {
            enabled: self.enabled,
            endpoint: self.endpoint.clone(),
            region: self.region.clone(),
            bucket: self.bucket.clone(),
            object_key: self.object_key.clone(),
            note_object_key: derive_note_object_key(&self.object_key),
            note_attachment_object_key: derive_note_attachment_object_key(&self.object_key),
            note_asset_prefix: derive_note_asset_prefix(&self.object_key),
            path_style: self.path_style,
            allow_http: self.allow_http,
            credentials_configured,
        }
    }
}

fn read_settings(connection: &Connection) -> Result<StoredSyncSettings, String> {
    connection
        .query_row(
            "
            SELECT enabled, endpoint, region, bucket, object_key, path_style, allow_http
            FROM sync_settings WHERE id = 1
            ",
            [],
            |row| {
                Ok(StoredSyncSettings {
                    enabled: row.get(0)?,
                    endpoint: row.get(1)?,
                    region: row.get(2)?,
                    bucket: row.get(3)?,
                    object_key: row.get(4)?,
                    path_style: row.get(5)?,
                    allow_http: row.get(6)?,
                })
            },
        )
        .map_err(|error| format!("读取同步配置失败：{error}"))
}

fn validate_credential_input(input: &SaveSyncSettings) -> Result<(), String> {
    match (&input.access_key, &input.secret_key) {
        (None, None) => Ok(()),
        (Some(access), Some(secret)) if !access.trim().is_empty() && !secret.is_empty() => Ok(()),
        _ => Err("Access Key 和 Secret Key 必须同时填写".to_string()),
    }
}

fn credential_entry(connection: &Connection) -> Result<Entry, String> {
    let user = device_id(connection).map_err(|error| format!("读取设备标识失败：{error}"))?;
    Entry::new(CREDENTIAL_SERVICE, &user).map_err(|error| format!("打开系统凭据库失败：{error}"))
}

fn credentials_configured(connection: &Connection) -> Result<bool, String> {
    match credential_entry(connection)?.get_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(error) => Err(format!("读取系统凭据状态失败：{error}")),
    }
}

fn store_credentials(
    connection: &Connection,
    access_key: &str,
    secret_key: &str,
) -> Result<(), String> {
    let encoded = serde_json::to_string(&StoredCredentials {
        access_key: access_key.trim().to_string(),
        secret_key: secret_key.to_string(),
    })
    .map_err(|error| format!("编码同步凭据失败：{error}"))?;
    credential_entry(connection)?
        .set_password(&encoded)
        .map_err(|error| format!("保存系统凭据失败：{error}"))
}

fn load_credentials(connection: &Connection) -> Result<Option<StoredCredentials>, String> {
    let encoded = match credential_entry(connection)?.get_password() {
        Ok(value) => value,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(error) => return Err(format!("读取系统凭据失败：{error}")),
    };
    serde_json::from_str(&encoded)
        .map(Some)
        .map_err(|_| "系统凭据格式无效，请删除后重新保存".to_string())
}

fn build_bucket(
    settings: &StoredSyncSettings,
    stored: StoredCredentials,
) -> Result<Box<Bucket>, String> {
    let region = if settings.endpoint.is_empty() {
        settings
            .region
            .parse::<Region>()
            .map_err(|error| format!("Region 无效：{error}"))?
    } else {
        Region::Custom {
            region: settings.region.clone(),
            endpoint: settings.endpoint.clone(),
        }
    };
    let credentials = Credentials::new(
        Some(&stored.access_key),
        Some(&stored.secret_key),
        None,
        None,
        None,
    )
    .map_err(|error| format!("创建 S3 凭据失败：{error}"))?;
    let bucket = Bucket::new(&settings.bucket, region, credentials)
        .map_err(|error| format!("创建 S3 客户端失败：{error}"))?;
    Ok(if settings.path_style {
        bucket.with_path_style()
    } else {
        bucket
    })
}

#[cfg(test)]
#[path = "s3_asset_safety_tests.rs"]
mod asset_safety_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn input(endpoint: &str, allow_http: bool) -> SaveSyncSettings {
        SaveSyncSettings {
            enabled: true,
            endpoint: endpoint.to_string(),
            region: "us-east-1".to_string(),
            bucket: "eggdone".to_string(),
            object_key: "/sync/todos.json".to_string(),
            path_style: true,
            allow_http,
            access_key: None,
            secret_key: None,
        }
    }

    #[test]
    fn rejects_http_without_explicit_confirmation() {
        let error =
            StoredSyncSettings::from_input(&input("http://127.0.0.1:9000", false)).unwrap_err();
        assert!(error.contains("明文传输"));
    }

    #[test]
    fn accepts_minio_http_after_confirmation() {
        let settings =
            StoredSyncSettings::from_input(&input("http://127.0.0.1:9000/", true)).unwrap();
        assert_eq!(settings.endpoint, "http://127.0.0.1:9000");
        assert_eq!(settings.object_key, "sync/todos.json");
    }

    #[test]
    fn accepts_aws_configuration_without_custom_endpoint() {
        let settings = StoredSyncSettings::from_input(&input("", false)).unwrap();
        assert!(settings.endpoint.is_empty());
    }

    #[test]
    fn requires_both_credential_fields() {
        let mut value = input("https://s3.example.com", false);
        value.access_key = Some("access".to_string());
        assert!(validate_credential_input(&value).is_err());
    }

    #[test]
    fn uses_etag_for_updates_and_create_guard_for_new_objects() {
        assert_eq!(
            upload_condition(Some("\"etag-value\"")),
            ("if-match", "\"etag-value\"")
        );
        assert_eq!(upload_condition(None), ("if-none-match", "*"));
    }

    #[test]
    fn retries_only_conditional_conflicts() {
        assert_eq!(
            classify_upload_status(412).unwrap(),
            UploadOutcome::Conflict
        );
        assert_eq!(
            classify_upload_status(409).unwrap(),
            UploadOutcome::Conflict
        );
        assert_eq!(classify_upload_status(200).unwrap(), UploadOutcome::Success);
        assert!(classify_upload_status(500).is_err());
    }

    #[test]
    fn derives_note_object_key_from_supported_todo_keys() {
        assert_eq!(
            derive_note_object_key("eggdone/todos.json"),
            "eggdone/notes.json"
        );
        assert_eq!(derive_note_object_key("backup.json"), "backup.notes.json");
        assert_eq!(derive_note_object_key("backup"), "backup.notes.json");
        assert_eq!(
            derive_note_object_key("egg.done/backup"),
            "egg.done/backup.notes.json"
        );
    }

    #[test]
    fn derives_note_attachment_keys_from_supported_todo_keys() {
        assert_eq!(
            derive_note_attachment_object_key("eggdone/todos.json"),
            "eggdone/note-attachments.json"
        );
        assert_eq!(
            derive_note_attachment_object_key("backup.json"),
            "backup.note-attachments.json"
        );
        assert_eq!(
            derive_note_attachment_object_key("folder/backup"),
            "folder/backup.note-attachments.json"
        );
        assert_eq!(
            derive_note_asset_prefix("eggdone/todos.json"),
            "eggdone/note-assets/v1/"
        );
        assert_eq!(
            derive_note_asset_prefix("backup.json"),
            "backup.note-assets/v1/"
        );
        assert_eq!(
            derive_note_asset_prefix("folder/backup"),
            "folder/backup.note-assets/v1/"
        );
    }

    #[test]
    fn runtime_rejects_overlapping_syncs_and_recovers_after_drop() {
        let runtime = SyncRuntime::default();
        let guard = runtime.acquire().unwrap();
        assert!(runtime.acquire().is_err());
        drop(guard);
        assert!(runtime.acquire().is_ok());
    }

    #[test]
    #[ignore = "explicit read-only diagnosis of a local migration backup"]
    fn diagnose_migration_assets_read_only() {
        let path = std::env::var("EGGDONE_MIGRATION_DIAGNOSTIC_DB")
            .expect("explicit database path required");
        let c =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let plan = crate::migration_backup::latest(&c)
            .unwrap()
            .expect("backup plan required");
        let source = prepare_migration_asset_source(&c).expect("source unavailable");
        let runtime = SyncRuntime::default();
        for (index, entry) in crate::migration_backup::asset_files(&plan)
            .iter()
            .enumerate()
        {
            let result = tauri::async_runtime::block_on(download_asset_bytes(
                &runtime,
                &source.0,
                &entry.name[..36],
                &entry.name[37..],
                entry.size as i64,
                &entry.sha256,
            ));
            let status = match &result {
                Ok(_) => "verified",
                Err(e) if e.contains("远端文件不存在") => "missing",
                Err(e) if e.contains("凭据") => "denied",
                Err(e) if e.contains("SHA-256") || e.contains("大小") => "integrity",
                Err(_) => "transport_or_response",
            };
            println!(
                "migration asset {} {}: {}",
                index + 1,
                &entry.name[37..],
                status
            );
        }
    }

    #[test]
    fn migration_asset_diagnostics_keep_cause_without_private_details() {
        assert_eq!(
            migration_asset_error("下载附件失败：远端文件不存在，可稍后重试同步"),
            "MIGRATION_BACKUP_ASSET_NOT_FOUND"
        );
        assert_eq!(
            migration_asset_error("下载附件失败：凭据无效或没有对象读取权限"),
            "MIGRATION_BACKUP_ASSET_DENIED"
        );
        assert_eq!(
            migration_asset_error("下载附件失败：SHA-256 校验不通过，请重试"),
            "MIGRATION_BACKUP_ASSET_INVALID"
        );
        assert_eq!(
            migration_asset_error("下载附件失败：文件大小与同步元数据不一致"),
            "MIGRATION_BACKUP_ASSET_INVALID"
        );
        assert_eq!(
            migration_asset_error("request error https://private.invalid/signed"),
            "MIGRATION_BACKUP_ASSET_DOWNLOAD"
        );
    }

    #[test]
    fn asset_runtime_deduplicates_each_attachment_uuid() {
        let runtime = SyncRuntime::default();
        let first_uuid = "6cb653ce-b48f-42d0-8409-d8df81ced98c";
        let second_uuid = "c04710a9-e9bc-49fd-9e30-4787044356d0";
        let first = runtime.acquire_asset(first_uuid).unwrap();
        assert!(runtime.acquire_asset(first_uuid).is_err());
        let second = runtime.acquire_asset(second_uuid).unwrap();
        drop(first);
        assert!(runtime.acquire_asset(first_uuid).is_ok());
        drop(second);
    }

    #[test]
    fn validates_asset_payload_and_remote_identity() {
        let bytes = b"eggdone attachment";
        let sha256 = sha256_hex(bytes);
        validate_asset_payload("original", bytes, "image/jpeg", &sha256).unwrap();
        assert!(validate_asset_payload("preview.png", bytes, "image/png", &sha256).is_err());
        assert!(validate_asset_payload("original", bytes, "image/jpeg", &"0".repeat(64)).is_err());

        let remote = RemoteAssetState {
            exists: true,
            etag: Some("\"fixture-etag\"".into()),
            content_length: Some(bytes.len() as i64),
            content_type: Some("image/jpeg".to_string()),
            sha256: Some(sha256.clone()),
        };
        assert!(asset_matches(
            &remote,
            bytes.len() as i64,
            "IMAGE/JPEG",
            &sha256
        ));
        assert!(!asset_matches_without_type(
            &remote,
            bytes.len() as i64 + 1,
            &sha256
        ));
    }
}
