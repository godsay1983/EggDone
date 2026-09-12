//! Independent link transport and read-only change probes.
use std::time::Duration;

use http::{HeaderMap, HeaderValue};
use s3::{
    bucket::Bucket, command::Command, request::tokio_backend::ReqwestRequest, request::Request,
};
use uuid::Uuid;

use crate::task_note_link_protocol::{self, LinkDocument};

pub const MAX_LINK_BYTES: usize = 4 * 1024 * 1024;

#[cfg(test)]
#[path = "task_note_link_s3_integration_tests.rs"]
mod s3_integration_tests;

pub struct TaskNoteLinkTransport {
    bucket: Box<Bucket>,
    object_key: String,
    owner: Uuid,
}

pub struct RemoteLinks {
    pub document: Option<LinkDocument>,
    etag: Option<String>,
    owner: Uuid,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LinkUploadOutcome {
    Uploaded { etag: String },
    Conflict,
}

fn valid_etag(value: &str) -> bool {
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
        .ok_or("TASK_NOTE_LINK_ETAG_REQUIRED")?;
    if values.next().is_some() {
        return Err("TASK_NOTE_LINK_ETAG_REQUIRED".to_string());
    }
    Ok(value.to_string())
}

impl TaskNoteLinkTransport {
    // A probe token detects change; it never authorizes a conditional upload.
    pub async fn probe(&self) -> Result<String, String> {
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, Command::HeadObject)
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        let response = request
            .response()
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        match response.status().as_u16() {
            200 => Ok(format!("etag:{}", response_etag(response.headers())?)),
            404 => Ok("missing".into()),
            403 => Ok("denied".into()),
            status => Err(format!("TASK_NOTE_LINK_HEAD_HTTP:{status}")),
        }
    }

    pub fn new(bucket: &Bucket, todo_key: &str, occupied_keys: &[String]) -> Result<Self, String> {
        let object_key = task_note_link_protocol::object_key(todo_key, occupied_keys)?;
        Ok(Self {
            bucket: bucket
                .with_request_timeout(Duration::from_secs(25))
                .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_SETUP")?,
            object_key,
            owner: Uuid::new_v4(),
        })
    }

    pub async fn download(&self) -> Result<RemoteLinks, String> {
        // Use the existing SDK signer but read chunks ourselves: get_object buffers without a cap.
        // One request only. Conflicts must restart the entire entity+link workflow, not retry a PUT.
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, Command::GetObject)
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        let mut response = request
            .response()
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        match response.status().as_u16() {
            404 => {
                return Ok(RemoteLinks {
                    document: None,
                    etag: None,
                    owner: self.owner,
                })
            }
            200 => (),
            status => return Err(format!("TASK_NOTE_LINK_DOWNLOAD_HTTP:{status}")),
        }
        let etag = response_etag(response.headers())?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_LINK_BYTES as u64)
        {
            return Err("TASK_NOTE_LINK_DOCUMENT_TOO_LARGE".to_string());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?
        {
            if chunk.len() > MAX_LINK_BYTES - bytes.len() {
                return Err("TASK_NOTE_LINK_DOCUMENT_TOO_LARGE".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| "INVALID_TASK_NOTE_LINK_UTF8")?;
        Ok(RemoteLinks {
            document: Some(task_note_link_protocol::parse_document(text)?),
            etag: Some(etag),
            owner: self.owner,
        })
    }

    pub async fn upload(
        &self,
        document: &LinkDocument,
        remote: &RemoteLinks,
    ) -> Result<LinkUploadOutcome, String> {
        // A cached ETag or a download from another client/configuration is not an upload token.
        if remote.owner != self.owner {
            return Err("TASK_NOTE_LINK_REMOTE_SCOPE_MISMATCH".to_string());
        }
        let content = task_note_link_protocol::encode_document(document)?;
        if content.len() > MAX_LINK_BYTES {
            return Err("TASK_NOTE_LINK_DOCUMENT_TOO_LARGE".to_string());
        }
        let mut headers = HeaderMap::new();
        let (name, value) = match remote.etag.as_deref() {
            Some(etag) => ("if-match", etag),
            None => ("if-none-match", "*"),
        };
        headers.insert(
            name,
            HeaderValue::from_str(value).map_err(|_| "TASK_NOTE_LINK_ETAG_REQUIRED")?,
        );
        let command = Command::PutObject {
            content: content.as_bytes(),
            content_type: "application/json",
            custom_headers: Some(headers),
            multipart: None,
        };
        let request = ReqwestRequest::new(&self.bucket, &self.object_key, command)
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        let response = request
            .response()
            .await
            .map_err(|_| "TASK_NOTE_LINK_TRANSPORT_NETWORK")?;
        match response.status().as_u16() {
            200 | 201 | 204 => Ok(LinkUploadOutcome::Uploaded {
                etag: response_etag(response.headers())?,
            }),
            409 | 412 => Ok(LinkUploadOutcome::Conflict),
            status => Err(format!("TASK_NOTE_LINK_UPLOAD_HTTP:{status}")),
        }
    }
}
