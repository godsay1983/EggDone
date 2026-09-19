//! Independent checklist transport and read-only change probes.
use std::time::Duration;

use http::{HeaderMap, HeaderValue};
use s3::{
    bucket::Bucket, command::Command, request::tokio_backend::ReqwestRequest, request::Request,
};
use uuid::Uuid;

use crate::task_checklist_sync::Domain;

// Share bounded conditional transport, not the checklist storage/ACK domain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WireDomain {
    Checklist(Domain),
    Templates,
    Planning,
}
impl WireDomain {
    fn canonical(self, raw: &str) -> Result<String, String> {
        match self {
            Self::Checklist(domain) => domain.canonical(raw),
            Self::Templates => {
                crate::task_template_protocol::encode(&crate::task_template_protocol::parse(raw)?)
            }
            Self::Planning => {
                crate::daily_plan_protocol::encode(&crate::daily_plan_protocol::parse(raw)?)
            }
        }
    }
}

pub const MAX_CHECKLIST_BYTES: usize = 4 * 1024 * 1024;

pub struct TaskChecklistTransport {
    bucket: Box<Bucket>,
    object_key: String,
    owner: Uuid,
    domain: WireDomain,
}

pub struct RemoteChecklist {
    pub document: Option<String>,
    etag: Option<String>,
    owner: Uuid,
    domain: WireDomain,
}

impl RemoteChecklist {
    pub(crate) fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChecklistUploadOutcome {
    Uploaded { etag: String },
    Conflict,
}

pub(crate) fn valid_etag(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=1024).contains(&bytes.len())
        && bytes.first() == Some(&b'"')
        && bytes.last() == Some(&b'"')
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|c| *c == 0x21 || (0x23..=0x7e).contains(c))
}

fn response_etag(headers: &HeaderMap) -> Result<String, String> {
    let mut values = headers.get_all("etag").iter();
    let value = values
        .next()
        .and_then(|v| v.to_str().ok())
        .filter(|v| valid_etag(v))
        .ok_or("CHECKLIST_ETAG_REQUIRED")?;
    if values.next().is_some() {
        return Err("CHECKLIST_ETAG_REQUIRED".to_string());
    }
    Ok(value.to_string())
}

impl TaskChecklistTransport {
    // A probe token detects change; it never authorizes a conditional upload.
    pub async fn probe(&self) -> Result<String, String> {
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, Command::HeadObject)
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        let response = request
            .response()
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        crate::sync_space::require_probe(&self.object_key, response.status().as_u16())?;
        match response.status().as_u16() {
            200 => Ok(format!("etag:{}", response_etag(response.headers())?)),
            404 => Ok("missing".into()),
            403 => Ok("denied".into()),
            status => Err(format!("CHECKLIST_HEAD_HTTP:{status}")),
        }
    }

    pub fn new(
        bucket: &Bucket,
        todo_key: &str,
        occupied_keys: &[String],
        domain: Domain,
    ) -> Result<Self, String> {
        let object_key = domain.object_key(todo_key, occupied_keys)?;
        Self::for_key(bucket, object_key, WireDomain::Checklist(domain))
    }

    pub fn templates(
        bucket: &Bucket,
        todo_key: &str,
        occupied_keys: &[String],
    ) -> Result<Self, String> {
        Self::for_key(
            bucket,
            crate::task_template_sync::object_key(todo_key, occupied_keys)?,
            WireDomain::Templates,
        )
    }

    pub fn planning(
        bucket: &Bucket,
        todo_key: &str,
        occupied_keys: &[String],
    ) -> Result<Self, String> {
        Self::for_key(
            bucket,
            crate::daily_plan_sync::object_key(todo_key, occupied_keys)?,
            WireDomain::Planning,
        )
    }

    fn for_key(bucket: &Bucket, object_key: String, domain: WireDomain) -> Result<Self, String> {
        Ok(Self {
            bucket: bucket
                .with_request_timeout(Duration::from_secs(25))
                .map_err(|_| "CHECKLIST_TRANSPORT_SETUP")?,
            object_key,
            owner: Uuid::new_v4(),
            domain,
        })
    }

    pub async fn download(&self) -> Result<RemoteChecklist, String> {
        // Use the existing SDK signer but read chunks ourselves: get_object buffers without a cap.
        // One request only. Conflicts must restart the entire entity+link workflow, not retry a PUT.
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, Command::GetObject)
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        let mut response = request
            .response()
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        crate::sync_space::require_probe(&self.object_key, response.status().as_u16())?;
        match response.status().as_u16() {
            404 => {
                crate::sync_space::require_existing(&self.object_key, None)?;
                return Ok(RemoteChecklist {
                    document: None,
                    etag: None,
                    owner: self.owner,
                    domain: self.domain,
                });
            }
            200 => (),
            status => return Err(format!("CHECKLIST_DOWNLOAD_HTTP:{status}")),
        }
        let etag = response_etag(response.headers())?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_CHECKLIST_BYTES as u64)
        {
            return Err("CHECKLIST_DOCUMENT_TOO_LARGE".to_string());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?
        {
            if chunk.len() > MAX_CHECKLIST_BYTES - bytes.len() {
                return Err("CHECKLIST_DOCUMENT_TOO_LARGE".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| "INVALID_CHECKLIST_UTF8")?;
        let text = crate::sync_space::decode(&self.object_key, text)?;
        Ok(RemoteChecklist {
            document: Some(
                self.domain
                    .canonical(&text)
                    .map_err(|_| "INVALID_CHECKLIST_DOCUMENT")?,
            ),
            etag: Some(etag),
            owner: self.owner,
            domain: self.domain,
        })
    }

    pub async fn upload(
        &self,
        document: &str,
        remote: &RemoteChecklist,
    ) -> Result<ChecklistUploadOutcome, String> {
        // A cached ETag or a download from another client/configuration is not an upload token.
        if remote.owner != self.owner || remote.domain != self.domain {
            return Err("CHECKLIST_REMOTE_SCOPE_MISMATCH".to_string());
        }
        let content = self.domain.canonical(document)?;
        crate::sync_space::require_existing(&self.object_key, remote.etag.as_deref())?;
        let content = crate::sync_space::encode(&self.object_key, &content)?;
        if content.len() > MAX_CHECKLIST_BYTES {
            return Err("CHECKLIST_DOCUMENT_TOO_LARGE".to_string());
        }
        let mut headers = HeaderMap::new();
        let (name, value) = match remote.etag.as_deref() {
            Some(etag) => ("if-match", etag),
            None => ("if-none-match", "*"),
        };
        headers.insert(
            name,
            HeaderValue::from_str(value).map_err(|_| "CHECKLIST_ETAG_REQUIRED")?,
        );
        let command = Command::PutObject {
            content: content.as_bytes(),
            content_type: "application/json",
            custom_headers: Some(headers),
            multipart: None,
        };
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, command)
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        let response = request
            .response()
            .await
            .map_err(|_| "CHECKLIST_TRANSPORT_NETWORK")?;
        match response.status().as_u16() {
            200 | 201 | 204 => Ok(ChecklistUploadOutcome::Uploaded {
                etag: response_etag(response.headers())?,
            }),
            409 | 412 => Ok(ChecklistUploadOutcome::Conflict),
            status => Err(format!("CHECKLIST_UPLOAD_HTTP:{status}")),
        }
    }
}
