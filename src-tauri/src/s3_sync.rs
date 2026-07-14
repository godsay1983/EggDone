use std::sync::atomic::{AtomicBool, Ordering};

use http::{HeaderMap, HeaderName, HeaderValue};
use keyring::{Entry, Error as KeyringError};
use rusqlite::{params, Connection};
use s3::{bucket::Bucket, creds::Credentials, region::Region};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    db::{device_id, now_millis},
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

pub struct PreparedManualSync {
    bucket: Box<Bucket>,
    object_key: String,
    note_object_key: String,
}

pub struct RemoteSyncObject {
    pub document: Option<SyncDocument>,
    etag: Option<String>,
}

pub struct RemoteNoteSyncObject {
    pub document: Option<NoteSyncDocument>,
    etag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncState {
    pub todo_object_exists: bool,
    pub todo_etag: Option<String>,
    pub note_object_exists: bool,
    pub note_etag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSyncResult {
    pub message: String,
    pub todo_count: usize,
    pub note_count: usize,
    pub conflict_retried: bool,
    pub todo_remote_etag: Option<String>,
    pub note_remote_etag: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UploadOutcome {
    Success,
    Conflict,
}

#[derive(Default)]
pub struct SyncRuntime {
    in_progress: AtomicBool,
}

pub struct SyncGuard<'a> {
    runtime: &'a SyncRuntime,
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

    get_settings(connection)
}

pub fn delete_credentials(connection: &Connection) -> Result<(), String> {
    let entry = credential_entry(connection)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {
            connection
                .execute(
                    "UPDATE sync_settings SET enabled = 0, updated_at = ?1 WHERE id = 1",
                    params![now_millis()],
                )
                .map_err(|error| format!("禁用同步失败：{error}"))?;
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
}

impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        self.runtime.in_progress.store(false, Ordering::Release);
    }
}

pub fn prepare_manual_sync(connection: &Connection) -> Result<PreparedManualSync, String> {
    let settings = read_settings(connection)?;
    if !settings.enabled {
        return Err("请先启用并保存同步配置".to_string());
    }
    settings.validate_connection()?;
    let credentials = load_credentials(connection)?
        .ok_or_else(|| "请先填写并保存 Access Key 和 Secret Key".to_string())?;
    Ok(PreparedManualSync {
        bucket: build_bucket(&settings, credentials)?,
        note_object_key: derive_note_object_key(&settings.object_key),
        object_key: settings.object_key,
    })
}

pub async fn download_remote(prepared: &PreparedManualSync) -> Result<RemoteSyncObject, String> {
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
        404 => Ok(RemoteSyncObject {
            document: None,
            etag: None,
        }),
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

    match status {
        200..=299 => {
            let etag = headers
                .e_tag
                .ok_or_else(|| "远端同步文件缺少 ETag".to_string())?;
            Ok((true, Some(etag)))
        }
        404 => Ok((false, None)),
        401 | 403 => Err("连接失败：凭据无效或没有 Bucket 访问权限".to_string()),
        _ => Err(format!("检查远端同步文件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn get_remote_state(prepared: &PreparedManualSync) -> Result<RemoteSyncState, String> {
    let (todo_object_exists, todo_etag) = get_object_state(prepared, &prepared.object_key).await?;
    let (note_object_exists, note_etag) =
        get_object_state(prepared, &prepared.note_object_key).await?;
    Ok(RemoteSyncState {
        todo_object_exists,
        todo_etag,
        note_object_exists,
        note_etag,
    })
}

pub async fn download_note_remote(
    prepared: &PreparedManualSync,
) -> Result<RemoteNoteSyncObject, String> {
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
        404 => Ok(RemoteNoteSyncObject {
            document: None,
            etag: None,
        }),
        401 | 403 => Err("下载便签失败：凭据无效或没有对象读取权限".to_string()),
        status => Err(format!("下载便签同步文件失败，S3 服务返回状态码 {status}")),
    }
}

pub async fn upload_document(
    prepared: &PreparedManualSync,
    document: &SyncDocument,
    remote: &RemoteSyncObject,
) -> Result<UploadOutcome, String> {
    let content = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("生成同步文件失败：{error}"))?;
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
}
