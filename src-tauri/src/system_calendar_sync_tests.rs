use super::*;
use crate::recurrence_transport::tests::{Reply, Server};
use std::{future::Future, sync::Arc, task::Poll};

const ACTIVE: &[u8] = include_bytes!("../../tests/fixtures/system-calendar-v1-active.json");
const WITHDRAWN: &[u8] = include_bytes!("../../tests/fixtures/system-calendar-v1-withdrawn.json");

struct Fixture {
    db: Arc<Database>,
    runtime: Arc<CalendarRuntime>,
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut connection).unwrap();
        connection.execute("UPDATE sync_settings SET enabled=1,endpoint='http://127.0.0.1:1',bucket='rules-test',region='test-region',object_key='account/todos.json',path_style=1,allow_http=1", []).unwrap();
        let directory =
            std::env::temp_dir().join(format!("eggdone-calendar-test-{}", uuid::Uuid::new_v4()));
        Self {
            db: Arc::new(Database {
                connection: Mutex::new(connection),
            }),
            runtime: Arc::new(CalendarRuntime::new(Some(directory.clone()))),
            directory,
        }
    }
    async fn refresh(&self, server: &Server) -> CalendarState {
        self.runtime
            .refresh_using(&self.db, |db| {
                s3_sync::prepare_with_fixture_credentials(db, server.bucket())
            })
            .await
    }
    fn restart(&self) -> CalendarRuntime {
        CalendarRuntime::new(Some(self.directory.clone()))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn assert_get(server: &Server, conditional: bool) {
    let request = server.request();
    assert!(request.head.starts_with(&format!(
        "GET /rules-test/{} ",
        system_calendar::object_key("account/todos.json")
    )));
    assert!(request.body.is_empty());
    let headers = request.head.to_lowercase();
    assert!(headers.contains("authorization: aws4-hmac-sha256"));
    assert_eq!(headers.contains("if-none-match: \"v1\""), conditional);
    assert!(!headers.contains("if-match:"));
    if conditional {
        assert!(headers
            .lines()
            .find(|line| line.starts_with("authorization:"))
            .unwrap()
            .contains("if-none-match"));
    }
}

#[test]
fn system_calendar_http_conditional_cache_restart_and_no_task_changes() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let snapshot = |db: &Database| {
            crate::sync_runtime_state::get_snapshot(&db.connection.lock().unwrap()).unwrap()
        };
        let before = serde_json::to_value(snapshot(&fixture.db)).unwrap();
        let server = Server::new(vec![
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(304, None, b""),
        ]);
        let state = fixture.refresh(&server).await;
        assert!(state.error.is_empty(), "{}", state.error);
        assert!(state.configured && !state.loading && state.last_received_at > 0);
        assert_eq!(state.document.as_ref().unwrap().occurrences.len(), 2);
        assert_get(&server, false);
        let restarted = fixture.restart();
        let hydrated = restarted.state(&fixture.db);
        assert_eq!(hydrated.document, state.document);
        let state = restarted
            .refresh_using(&fixture.db, |db| {
                s3_sync::prepare_with_fixture_credentials(db, server.bucket())
            })
            .await;
        assert!(state.error.is_empty());
        assert_eq!(state.document, hydrated.document);
        assert_get(&server, true);
        assert_eq!(serde_json::to_value(snapshot(&fixture.db)).unwrap(), before);
    });
}

#[test]
fn system_calendar_http_missing_denied_invalid_transient_and_withdrawn() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let server = Server::new(vec![
            Reply::new(404, None, b""),
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(403, None, b"sensitive credential error body"),
            Reply::new(503, None, b"secret endpoint"),
            Reply::new(200, Some("\"v2\""), b"bad json"),
            Reply::new(404, None, b""),
            Reply::new(404, None, b""),
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(200, Some("\"withdrawn\""), WITHDRAWN),
            Reply::new(304, None, b""),
        ]);
        let state = fixture.refresh(&server).await;
        assert!(state.document.is_none() && state.error.is_empty());
        let active = fixture.refresh(&server).await;
        for expected in [
            "CALENDAR_DENIED",
            "CALENDAR_NETWORK",
            "CALENDAR_INVALID_DOCUMENT",
        ] {
            let state = fixture.refresh(&server).await;
            assert_eq!(state.error, expected);
            assert_eq!(state.document, active.document);
            assert_eq!(state.last_received_at, active.last_received_at);
        }
        for _ in 0..2 {
            let state = fixture.refresh(&server).await;
            assert_eq!(state.error, "CALENDAR_SOURCE_MISSING");
            assert!(state.document.is_none());
        }
        assert_eq!(
            fixture.restart().state(&fixture.db).error,
            "CALENDAR_SOURCE_MISSING"
        );
        assert!(fixture.refresh(&server).await.document.is_some());
        for _ in 0..2 {
            let state = fixture.refresh(&server).await;
            assert_eq!(
                state.document.as_ref().unwrap().state,
                CalendarStatus::Withdrawn
            );
            assert!(state.document.as_ref().unwrap().occurrences.is_empty());
            assert!(state.error.is_empty(), "{}", state.error);
        }
        assert_eq!(
            fixture.restart().state(&fixture.db).document.unwrap().state,
            CalendarStatus::Withdrawn
        );
    });
}

#[test]
fn system_calendar_http_strong_tags_and_bounded_responses() {
    tauri::async_runtime::block_on(async {
        for (reply, code) in [
            (Reply::new(200, None, ACTIVE), "CALENDAR_ETAG_REQUIRED"),
            (
                Reply::new(200, Some("W/\"weak\""), ACTIVE),
                "CALENDAR_ETAG_REQUIRED",
            ),
            (
                Reply::new(200, Some("unquoted"), ACTIVE),
                "CALENDAR_ETAG_REQUIRED",
            ),
            (
                Reply::new(200, Some("\"one\""), ACTIVE).with_header("ETag", "\"two\""),
                "CALENDAR_ETAG_REQUIRED",
            ),
            (Reply::new(304, None, b""), "CALENDAR_INVALID_RESPONSE"),
            (
                Reply::new(302, None, b"").with_header("Location", "http://127.0.0.1:1/private"),
                "CALENDAR_INVALID_RESPONSE",
            ),
            (
                Reply::new(200, Some("\"v1\""), &vec![b' '; MAX_BYTES + 1]),
                "CALENDAR_DOCUMENT_TOO_LARGE",
            ),
            (
                Reply::new(200, Some("\"v1\""), &vec![b' '; MAX_BYTES + 1]).with_chunked_body(),
                "CALENDAR_DOCUMENT_TOO_LARGE",
            ),
            (
                Reply::new(200, Some("\"v1\""), b"\xff"),
                "CALENDAR_INVALID_DOCUMENT",
            ),
        ] {
            let fixture = Fixture::new();
            let server = Server::new(vec![reply]);
            assert_eq!(fixture.refresh(&server).await.error, code);
            assert_get(&server, false);
        }
        let fixture = Fixture::new();
        let server = Server::new(vec![
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(304, Some("\"different\""), b""),
        ]);
        let old = fixture.refresh(&server).await;
        let next = fixture.refresh(&server).await;
        assert_eq!(next.error, "CALENDAR_INVALID_RESPONSE");
        assert_eq!(next.document, old.document);
    });
}

#[test]
fn system_calendar_http_revision_regression_and_takeover() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let mut doc: serde_json::Value = serde_json::from_slice(ACTIVE).unwrap();
        doc["revision"] = 2.into();
        let second = serde_json::to_vec(&doc).unwrap();
        doc["revision"] = 1.into();
        doc["owner_generation"] = "44444444-4444-4444-8444-444444444444".into();
        let takeover = serde_json::to_vec(&doc).unwrap();
        let server = Server::new(vec![
            Reply::new(200, Some("\"v2\""), &second),
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(200, Some("\"takeover\""), &takeover),
        ]);
        assert_eq!(fixture.refresh(&server).await.document.unwrap().revision, 2);
        let state = fixture.refresh(&server).await;
        assert_eq!(state.error, "CALENDAR_REVISION_REGRESSION");
        assert_eq!(state.document.unwrap().revision, 2);
        let state = fixture.refresh(&server).await;
        assert!(state.error.is_empty());
        assert_eq!(state.document.unwrap().revision, 1);
    });
}

#[test]
fn system_calendar_target_identity_hides_memory_and_persisted_cache() {
    tauri::async_runtime::block_on(async {
        for sql in [
            "UPDATE sync_settings SET endpoint='http://127.0.0.1:2'",
            "UPDATE sync_settings SET bucket='another'",
            "UPDATE sync_settings SET region='another'",
            "UPDATE sync_settings SET object_key='other/todos.json'",
            "UPDATE sync_settings SET path_style=0",
            "UPDATE sync_settings SET enabled=0",
            "UPDATE sync_settings SET allow_http=0",
            "UPDATE app_metadata SET value='pending:changed' WHERE key='sync.target.epoch.v1'",
            "UPDATE app_metadata SET value='credential-change' WHERE key='sync.target.epoch.v1'",
        ] {
            let fixture = Fixture::new();
            let server = Server::new(vec![Reply::new(200, Some("\"v1\""), ACTIVE)]);
            assert!(fixture.refresh(&server).await.document.is_some());
            fixture
                .db
                .connection
                .lock()
                .unwrap()
                .execute(sql, [])
                .unwrap();
            // Check startup hydration before the in-memory session gets a chance to remove the file.
            assert!(
                fixture.restart().state(&fixture.db).document.is_none(),
                "{sql}"
            );
            let state = fixture.runtime.state(&fixture.db);
            assert!(
                state.document.is_none() && state.last_received_at == 0 && !state.loading,
                "{sql}"
            );
        }
    });
}

#[test]
fn system_calendar_late_http_response_never_restores_old_target() {
    tauri::async_runtime::block_on(async {
        for status in [200, 304, 403, 404, 503] {
            let fixture = Fixture::new();
            let target = fixture.db.clone();
            let server = Server::new(vec![
                Reply::new(200, Some("\"v1\""), ACTIVE),
                Reply::new(status, Some("\"v1\""), ACTIVE).with_hook(move || {
                    let db = target.connection.lock().unwrap();
                    crate::sync_target::invalidate(&db).unwrap();
                    crate::sync_target::activate(&db).unwrap();
                }),
            ]);
            assert!(fixture.refresh(&server).await.document.is_some());
            let state = fixture.refresh(&server).await;
            assert!(state.document.is_none() && state.error.is_empty() && !state.loading);
            assert_eq!(state.last_received_at, 0);
            assert!(fixture.restart().state(&fixture.db).document.is_none());
        }
    });
}

#[test]
fn system_calendar_concurrent_refreshes_coalesce_without_task_lock() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let task_runtime = s3_sync::SyncRuntime::default();
        let _task_job = task_runtime.acquire().unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let server = Server::new(vec![Reply::new(200, Some("\"v1\""), ACTIVE).with_hook(
            move || {
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
            },
        )]);
        let runtime = fixture.runtime.clone();
        let db = fixture.db.clone();
        let bucket = server.bucket();
        let first = tauri::async_runtime::spawn(async move {
            runtime
                .refresh_using(&db, |db| {
                    s3_sync::prepare_with_fixture_credentials(db, bucket)
                })
                .await
        });
        assert_get(&server, false);
        assert!(fixture.runtime.state(&fixture.db).loading);
        let mut second = Box::pin(fixture.refresh(&server));
        std::future::poll_fn(|context| {
            assert!(second.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        release.send(()).unwrap();
        let first = first.await.unwrap();
        let second = second.await;
        assert!(first.error.is_empty() && second.error.is_empty());
        assert_eq!(first.document, second.document);
        assert_eq!(first.last_received_at, second.last_received_at);
    });
}

#[test]
fn system_calendar_corrupt_cache_and_disabled_target_are_safe() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let server = Server::new(vec![Reply::new(200, Some("\"v1\""), ACTIVE)]);
        assert!(fixture.refresh(&server).await.document.is_some());
        fs::write(fixture.runtime.cache_path.as_ref().unwrap(), b"broken").unwrap();
        assert!(fixture.restart().state(&fixture.db).document.is_none());
        fixture
            .db
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE sync_settings SET enabled=0", [])
            .unwrap();
        let state = fixture
            .runtime
            .refresh_using(&fixture.db, |_| {
                panic!("disabled must not read credentials or send HTTP")
            })
            .await;
        assert!(!state.configured && state.error.is_empty() && state.document.is_none());
    });
}

#[test]
fn system_calendar_404_restart_preserves_revision_floor_without_content() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let mut doc: serde_json::Value = serde_json::from_slice(ACTIVE).unwrap();
        doc["revision"] = 2.into();
        let newer = serde_json::to_vec(&doc).unwrap();
        let server = Server::new(vec![
            Reply::new(200, Some("\"v2\""), &newer),
            Reply::new(404, None, b""),
            Reply::new(200, Some("\"v1\""), ACTIVE),
        ]);
        fixture.refresh(&server).await;
        assert_eq!(
            fixture.refresh(&server).await.error,
            "CALENDAR_SOURCE_MISSING"
        );
        let persisted = fs::read_to_string(fixture.runtime.cache_path.as_ref().unwrap()).unwrap();
        assert!(
            !persisted.contains("Read-only meeting") && !persisted.contains("Example calendar")
        );
        let restarted = fixture.restart();
        let state = restarted
            .refresh_using(&fixture.db, |db| {
                s3_sync::prepare_with_fixture_credentials(db, server.bucket())
            })
            .await;
        assert_eq!(state.error, "CALENDAR_REVISION_REGRESSION");
        assert!(state.document.is_none());
    });
}

#[test]
fn system_calendar_equal_revision_mutation_is_rejected_and_credentials_errors_are_private() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let mut doc: serde_json::Value = serde_json::from_slice(ACTIVE).unwrap();
        doc["occurrences"][0]["title"] = "Changed without revision".into();
        let server = Server::new(vec![
            Reply::new(200, Some("\"v1\""), ACTIVE),
            Reply::new(200, Some("\"mutated\""), &serde_json::to_vec(&doc).unwrap()),
        ]);
        let first = fixture.refresh(&server).await;
        let state = fixture.refresh(&server).await;
        assert_eq!(state.error, "CALENDAR_REVISION_REGRESSION");
        assert_eq!(state.document, first.document);
        let state = fixture
            .runtime
            .refresh_using(&fixture.db, |_| Err("keyring secret diagnostic".into()))
            .await;
        assert_eq!(state.error, "CALENDAR_CREDENTIALS");
        assert_eq!(state.document, first.document);
    });
}

#[test]
fn system_calendar_network_failure_keeps_cache_and_unconfigured_does_no_work() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let server = Server::new(vec![Reply::new(200, Some("\"v1\""), ACTIVE)]);
        let first = fixture.refresh(&server).await;
        let closed_bucket = server.bucket();
        drop(server);
        let state = fixture
            .runtime
            .refresh_using(&fixture.db, |db| {
                s3_sync::prepare_with_fixture_credentials(db, closed_bucket)
            })
            .await;
        assert_eq!(state.error, "CALENDAR_NETWORK");
        assert_eq!(state.document, first.document);
        assert_eq!(state.last_received_at, first.last_received_at);
        fixture
            .db
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE sync_settings SET enabled=0", [])
            .unwrap();
        let state = fixture
            .runtime
            .refresh_using(&fixture.db, |_| panic!("unconfigured"))
            .await;
        assert!(!state.configured && !state.loading && state.document.is_none());
        assert!(fixture.restart().state(&fixture.db).document.is_none());
    });
}

#[test]
fn system_calendar_empty_strong_etag_is_valid_and_conditional() {
    tauri::async_runtime::block_on(async {
        let fixture = Fixture::new();
        let server = Server::new(vec![
            Reply::new(200, Some("\"\""), ACTIVE),
            Reply::new(304, Some("\"\""), b""),
        ]);
        let state = fixture.refresh(&server).await;
        assert!(state.error.is_empty());
        assert!(!server
            .request()
            .head
            .to_lowercase()
            .contains("if-none-match:"));
        let next = fixture.refresh(&server).await;
        assert!(next.error.is_empty());
        assert_eq!(next.document, state.document);
        assert!(server
            .request()
            .head
            .to_lowercase()
            .contains("if-none-match: \"\""));
    });
}

#[test]
fn system_calendar_disabling_pending_config_clears_its_error() {
    let fixture = Fixture::new();
    {
        let db = fixture.db.connection.lock().unwrap();
        crate::sync_target::invalidate(&db).unwrap();
    }
    assert_eq!(
        fixture.runtime.state(&fixture.db).error,
        "CALENDAR_TARGET_CHANGED"
    );
    fixture
        .db
        .connection
        .lock()
        .unwrap()
        .execute("UPDATE sync_settings SET enabled=0", [])
        .unwrap();
    let state = fixture.runtime.state(&fixture.db);
    assert!(!state.configured && state.document.is_none() && state.error.is_empty());
}
