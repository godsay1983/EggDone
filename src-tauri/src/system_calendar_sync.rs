//! Receive-only calendar session. No task-sync lock, task storage, PUT or DELETE.
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

use http::{HeaderMap, HeaderValue};
use s3::{
    bucket::Bucket,
    command::Command,
    request::{tokio_backend::ReqwestRequest, Request},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    db::{now_millis, Database},
    s3_sync,
    system_calendar::{self, CalendarDocument, CalendarStatus, MAX_BYTES},
};

#[derive(Clone, Default, Debug, Serialize)]
pub struct CalendarState {
    pub document: Option<CalendarDocument>,
    pub loading: bool,
    pub error: String,
    pub last_received_at: i64,
    pub configured: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cache {
    version: u8,
    binding: String,
    document: Option<CalendarDocument>,
    etag: Option<String>,
    seen: bool,
    last_received_at: i64,
    cursor: RevisionCursor,
}

// Keep only non-content monotonic evidence when a previously observed object disappears.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RevisionCursor {
    owner_id: String,
    owner_generation: String,
    revision: i64,
    digest: String,
}

fn cursor(document: &CalendarDocument) -> Result<RevisionCursor, String> {
    let bytes = serde_json::to_vec(document).map_err(|_| "CALENDAR_INVALID_DOCUMENT")?;
    Ok(RevisionCursor {
        owner_id: document.owner_id.clone(),
        owner_generation: document.owner_generation.clone(),
        revision: document.revision,
        digest: format!("{:x}", Sha256::digest(&bytes)),
    })
}

#[derive(Default)]
struct Session {
    initialized: bool,
    binding: Option<String>,
    cache: Option<Cache>,
    loading: bool,
    error: String,
    completed: u64,
}

pub struct CalendarRuntime {
    session: Mutex<Session>,
    job: tauri::async_runtime::Mutex<()>,
    cache_path: Option<PathBuf>,
}

impl CalendarRuntime {
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        Self {
            session: Mutex::new(Session::default()),
            job: tauri::async_runtime::Mutex::new(()),
            cache_path: cache_dir.map(|p| p.join("system-calendar-v1").join("receiver.json")),
        }
    }

    // Caller holds DB first, then the calendar mutex. Never hold either across network I/O.
    fn reconcile(&self, session: &mut Session, db: &rusqlite::Connection) {
        let target = s3_sync::calendar_target_identity(db);
        let binding = target.as_ref().ok().cloned().flatten();
        if !session.initialized || session.binding != binding {
            let initial = !session.initialized;
            session.initialized = true;
            session.binding = binding.clone();
            session.cache = None;
            session.loading = false;
            session.error.clear();
            // Hydration is permitted only for this exact epoch AND configuration, even on restart.
            if initial {
                if let Some(binding) = &binding {
                    session.cache = self.read_cache(binding);
                    if session.cache.as_ref().is_some_and(|c| c.document.is_none()) {
                        session.error = "CALENDAR_SOURCE_MISSING".into();
                    }
                }
            }
            if session.cache.is_none() {
                self.remove_cache();
            }
        }
        match target {
            Err(error) => session.error = error,
            Ok(None) => session.error.clear(),
            Ok(Some(_)) => (),
        }
    }

    pub(crate) fn state_for_connection(&self, db: &rusqlite::Connection) -> CalendarState {
        let Ok(mut session) = self.session.lock() else {
            return failure("CALENDAR_INTERNAL");
        };
        self.reconcile(&mut session, db);
        view(&session)
    }

    pub fn state(&self, db: &Database) -> CalendarState {
        match db.connection.lock() {
            Ok(connection) => self.state_for_connection(&connection),
            Err(_) => failure("CALENDAR_DATABASE"),
        }
    }

    pub async fn refresh(&self, db: &Database) -> CalendarState {
        self.refresh_using(db, s3_sync::prepare_manual_sync).await
    }

    async fn refresh_using(
        &self,
        db: &Database,
        prepare: impl FnOnce(&rusqlite::Connection) -> Result<s3_sync::PreparedManualSync, String>,
    ) -> CalendarState {
        let ticket = match db.connection.lock() {
            Ok(connection) => match self.session.lock() {
                Ok(mut session) => {
                    self.reconcile(&mut session, &connection);
                    (session.binding.clone(), session.completed)
                }
                Err(_) => return failure("CALENDAR_INTERNAL"),
            },
            Err(_) => return failure("CALENDAR_DATABASE"),
        };
        let _job = self.job.lock().await;
        let (binding, prepared, etag) = {
            let Ok(connection) = db.connection.lock() else {
                return failure("CALENDAR_DATABASE");
            };
            let Ok(mut session) = self.session.lock() else {
                return failure("CALENDAR_INTERNAL");
            };
            self.reconcile(&mut session, &connection);
            if session.binding.is_none()
                || (ticket.0 == session.binding && ticket.1 != session.completed)
            {
                return view(&session);
            }
            let prepared = match prepare(&connection) {
                Ok(prepared) => prepared,
                Err(_) => {
                    session.error = "CALENDAR_CREDENTIALS".into();
                    session.completed = session.completed.wrapping_add(1);
                    return view(&session);
                }
            };
            session.loading = true;
            session.error.clear();
            let etag = session.cache.as_ref().and_then(|c| c.etag.clone());
            (session.binding.clone(), prepared, etag)
        };
        let result = download(
            prepared.calendar_bucket(),
            prepared.main_key(),
            etag.as_deref(),
        )
        .await;
        let Ok(connection) = db.connection.lock() else {
            return failure("CALENDAR_DATABASE");
        };
        let Ok(mut session) = self.session.lock() else {
            return failure("CALENDAR_INTERNAL");
        };
        self.reconcile(&mut session, &connection);
        if session.binding != binding {
            // A late response may not alter the new target's error, receipt, cache or loading state.
            return view(&session);
        }
        session.loading = false;
        session.completed = session.completed.wrapping_add(1);
        let result = apply(&mut session, result, now_millis());
        if let Err(error) = result {
            session.error = error;
        } else if let Some(cache) = &session.cache {
            if cache
                .document
                .as_ref()
                .is_none_or(|d| d.state == CalendarStatus::Withdrawn)
            {
                self.remove_cache();
            }
            if self.write_cache(cache).is_err() {
                session.error = "CALENDAR_CACHE".into();
            }
        }
        view(&session)
    }

    fn read_cache(&self, binding: &str) -> Option<Cache> {
        let file = fs::File::open(self.cache_path.as_ref()?).ok()?;
        let mut bytes = Vec::new();
        file.take((MAX_BYTES + 16_385) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_BYTES + 16_384 {
            return None;
        }
        let cache: Cache = serde_json::from_slice(&bytes).ok()?;
        if cache.version != 1
            || cache.binding != binding
            || !cache.seen
            || !(1..=system_calendar::MAX_SAFE_INTEGER).contains(&cache.last_received_at)
            || cache.document.is_some() != cache.etag.is_some()
            || cache.etag.as_deref().is_some_and(|tag| !valid_etag(tag))
        {
            return None;
        }
        if let Some(document) = &cache.document {
            let encoded = serde_json::to_vec(document).ok()?;
            system_calendar::parse(&encoded).ok()?;
            if cursor(document).ok()? != cache.cursor {
                return None;
            }
        } else if uuid::Uuid::parse_str(&cache.cursor.owner_id).is_err()
            || uuid::Uuid::parse_str(&cache.cursor.owner_generation).is_err()
            || !(1..=system_calendar::MAX_SAFE_INTEGER).contains(&cache.cursor.revision)
            || cache.cursor.digest.len() != 64
            || !cache
                .cursor
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        Some(cache)
    }

    fn write_cache(&self, cache: &Cache) -> Result<(), ()> {
        let path = self.cache_path.as_ref().ok_or(())?;
        let parent = path.parent().ok_or(())?;
        fs::create_dir_all(parent).map_err(|_| ())?;
        let pending = path.with_extension("tmp");
        let bytes = serde_json::to_vec(cache).map_err(|_| ())?;
        let mut file = fs::File::create(&pending).map_err(|_| ())?;
        file.write_all(&bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        // Windows rename cannot replace. Losing this disposable cache is safer than stale reuse.
        if path.exists() {
            fs::remove_file(path).map_err(|_| ())?;
        }
        fs::rename(pending, path).map_err(|_| ())
    }

    fn remove_cache(&self) {
        if let Some(path) = &self.cache_path {
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(path.with_extension("tmp"));
        }
    }
}

fn failure(code: &str) -> CalendarState {
    CalendarState {
        error: code.into(),
        ..CalendarState::default()
    }
}

fn view(session: &Session) -> CalendarState {
    CalendarState {
        document: session
            .cache
            .as_ref()
            .and_then(|c| c.document.as_ref())
            .cloned(),
        loading: session.loading,
        error: session.error.clone(),
        last_received_at: session.cache.as_ref().map_or(0, |c| c.last_received_at),
        configured: session.binding.is_some(),
    }
}

enum Download {
    Missing,
    Unchanged,
    Document(CalendarDocument, String),
}

fn apply(session: &mut Session, result: Result<Download, String>, now: i64) -> Result<(), String> {
    match result? {
        Download::Missing => {
            let seen = session.cache.as_ref().is_some_and(|c| c.seen);
            if let Some(cache) = &mut session.cache {
                cache.document = None;
                cache.etag = None;
                cache.last_received_at = now;
            }
            session.error = if seen {
                "CALENDAR_SOURCE_MISSING".into()
            } else {
                String::new()
            };
        }
        Download::Unchanged => {
            let cache = session
                .cache
                .as_mut()
                .filter(|c| c.document.is_some() && c.etag.is_some())
                .ok_or("CALENDAR_INVALID_RESPONSE")?;
            cache.last_received_at = now;
            session.error.clear();
        }
        Download::Document(document, etag) => {
            let incoming = cursor(&document)?;
            if let Some(previous) = session.cache.as_ref().map(|c| &c.cursor) {
                if previous.owner_id == incoming.owner_id
                    && previous.owner_generation == incoming.owner_generation
                    && (incoming.revision < previous.revision
                        || (incoming.revision == previous.revision
                            && incoming.digest != previous.digest))
                {
                    return Err("CALENDAR_REVISION_REGRESSION".into());
                }
            }
            session.cache = Some(Cache {
                version: 1,
                binding: session.binding.clone().unwrap_or_default(),
                document: Some(document),
                etag: Some(etag),
                seen: true,
                last_received_at: now,
                cursor: incoming,
            });
            session.error.clear();
        }
    }
    Ok(())
}

fn valid_etag(value: &str) -> bool {
    // RFC 9110 entity-tag: an empty opaque value is a valid strong ETag.
    let bytes = value.as_bytes();
    (2..=1024).contains(&bytes.len())
        && bytes.first() == Some(&b'"')
        && bytes.last() == Some(&b'"')
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|b| *b == 0x21 || (0x23..=0x7e).contains(b))
}

fn response_etag(headers: &HeaderMap) -> Result<String, String> {
    let mut tags = headers.get_all("etag").iter();
    let tag = tags
        .next()
        .and_then(|v| v.to_str().ok())
        .filter(|v| valid_etag(v))
        .ok_or("CALENDAR_ETAG_REQUIRED")?;
    if tags.next().is_some() {
        return Err("CALENDAR_ETAG_REQUIRED".into());
    }
    Ok(tag.into())
}

async fn download(bucket: &Bucket, todo_key: &str, etag: Option<&str>) -> Result<Download, String> {
    let mut bucket = bucket
        .with_request_timeout(Duration::from_secs(25))
        .map_err(|_| "CALENDAR_NETWORK")?;
    bucket.extra_headers.remove("if-match");
    bucket.extra_headers.remove("if-none-match");
    if let Some(etag) = etag {
        if !valid_etag(etag) {
            return Err("CALENDAR_ETAG_REQUIRED".into());
        }
        bucket.extra_headers.insert(
            "if-none-match",
            HeaderValue::from_str(etag).map_err(|_| "CALENDAR_ETAG_REQUIRED")?,
        );
    }
    let key = system_calendar::object_key(todo_key);
    let request = ReqwestRequest::new(&bucket, &key, Command::GetObject)
        .await
        .map_err(|_| "CALENDAR_NETWORK")?;
    // Reuse SigV4, but do not follow redirects to another object or forward signed headers.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "CALENDAR_NETWORK")?;
    let headers = request.headers().await.map_err(|_| "CALENDAR_NETWORK")?;
    let mut response = client
        .get(request.url().map_err(|_| "CALENDAR_NETWORK")?)
        .headers(headers)
        .send()
        .await
        .map_err(|_| "CALENDAR_NETWORK")?;
    match response.status().as_u16() {
        404 => return Ok(Download::Missing),
        401 | 403 => return Err("CALENDAR_DENIED".into()),
        304 => {
            let Some(expected) = etag else {
                return Err("CALENDAR_INVALID_RESPONSE".into());
            };
            if response.headers().contains_key("etag")
                && response_etag(response.headers())? != expected
            {
                return Err("CALENDAR_INVALID_RESPONSE".into());
            }
            return Ok(Download::Unchanged);
        }
        200 => (),
        500..=599 | 408 | 429 => return Err("CALENDAR_NETWORK".into()),
        _ => return Err("CALENDAR_INVALID_RESPONSE".into()),
    }
    let etag = response_etag(response.headers())?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BYTES as u64)
    {
        return Err("CALENDAR_DOCUMENT_TOO_LARGE".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "CALENDAR_NETWORK")? {
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err("CALENDAR_DOCUMENT_TOO_LARGE".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Download::Document(system_calendar::parse(&bytes)?, etag))
}

#[cfg(test)]
#[path = "system_calendar_sync_tests.rs"]
mod tests;
