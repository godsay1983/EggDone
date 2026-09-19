use super::*;
use crate::recurrence_transport::tests::{Reply, Server};

const ID: &str = "00000000-0000-4000-8000-000000000002";

fn prepared(server: &Server) -> PreparedManualSync {
    PreparedManualSync {
        target_epoch: "test".into(),
        bucket: server.bucket(),
        object_key: "account/todos.json".into(),
        note_object_key: "account/notes.json".into(),
        note_attachment_object_key: "account/note-attachments.json".into(),
        note_asset_prefix: "account/note-assets/v1/".into(),
    }
}
fn head(etag: Option<&str>) -> Reply {
    Reply::new(200, etag, b"data").with_header("x-amz-meta-sha256", &sha256_hex(b"data"))
}

#[test]
fn asset_cleanup_sends_signed_cas_and_never_falls_back_after_conflict() {
    tauri::async_runtime::block_on(async {
        let runtime = SyncRuntime::default();
        for status in [204, 404, 409, 412, 403, 501, 202, 206] {
            let server = Server::new(vec![
                head(Some("\"before\"")),
                Reply::new(status, None, b""),
            ]);
            let target = prepared(&server);
            let result =
                delete_asset_if_matches(&runtime, &target, ID, "original", 4, &sha256_hex(b"data"))
                    .await;
            assert_eq!(result.is_ok(), [204, 404].contains(&status));
            let head = server.request();
            assert!(head.head.starts_with("HEAD "));
            let delete = server.request().head.to_lowercase();
            assert!(delete.starts_with(&format!(
                "delete /rules-test/account/note-assets/v1/{ID}/original "
            )));
            assert!(delete.contains("if-match: \"before\""));
            let signed = delete
                .lines()
                .find(|s| s.starts_with("authorization:"))
                .unwrap();
            assert!(signed.contains("if-match"));
            assert!(target.bucket.extra_headers.get("if-match").is_none());
        }
    });
}

#[test]
fn asset_cleanup_refuses_missing_weak_wildcard_or_multiple_etags() {
    tauri::async_runtime::block_on(async {
        let runtime = SyncRuntime::default();
        for etag in [
            None,
            Some("*"),
            Some(" * "),
            Some("W/\"weak\""),
            Some("\"a\", \"b\""),
        ] {
            let server = Server::new(vec![head(etag)]);
            assert!(delete_asset_if_matches(
                &runtime,
                &prepared(&server),
                ID,
                "original",
                4,
                &sha256_hex(b"data")
            )
            .await
            .is_err());
            assert!(server.request().head.starts_with("HEAD "));
        }
        let server = Server::new(vec![Reply::new(404, None, b"")]);
        assert!(!delete_asset_if_matches(
            &runtime,
            &prepared(&server),
            ID,
            "original",
            4,
            &sha256_hex(b"data")
        )
        .await
        .unwrap());
    });
}

#[test]
fn migration_source_only_reads_exact_asset_and_redacts_errors() {
    tauri::async_runtime::block_on(async {
        for (status, body, success) in [
            (200, b"data".as_slice(), true),
            (200, b"xxxx".as_slice(), false),
            (404, b"".as_slice(), false),
            (403, b"private-error".as_slice(), false),
        ] {
            let server = Server::new(vec![Reply::new(status, None, body)]);
            let source = MigrationAssetSource(prepared(&server));
            let result = source
                .download(
                    &SyncRuntime::default(),
                    ID,
                    "original",
                    4,
                    &sha256_hex(b"data"),
                )
                .await;
            if success {
                assert_eq!(result.unwrap(), b"data");
            } else {
                let code = match status {
                    404 => "NOT_FOUND",
                    403 => "DENIED",
                    _ => "INVALID",
                };
                assert_eq!(
                    result.unwrap_err(),
                    format!("MIGRATION_BACKUP_ASSET_{code}:{ID}:original")
                );
            }
            assert!(server.request().head.starts_with(&format!(
                "GET /rules-test/account/note-assets/v1/{ID}/original "
            )));
        }
    });
}

#[test]
fn historical_asset_without_hash_metadata_requires_bound_content_verification() {
    tauri::async_runtime::block_on(async {
        for (bytes, tag, ok) in [
            (b"data", "\"same\"", true),
            (b"xxxx", "\"same\"", false),
            (b"data", "\"new\"", false),
        ] {
            let mut replies = vec![
                Reply::new(200, Some("\"same\""), b"data"),
                Reply::new(200, Some(tag), bytes),
            ];
            if ok {
                replies.push(Reply::new(204, None, b""));
            }
            let server = Server::new(replies);
            let result = delete_asset_if_matches(
                &SyncRuntime::default(),
                &prepared(&server),
                ID,
                "original",
                4,
                &sha256_hex(b"data"),
            )
            .await;
            assert_eq!(result.is_ok(), ok);
            assert!(server.request().head.starts_with("HEAD "));
            let read = server.request().head.to_lowercase();
            assert!(read.starts_with("get ") && read.contains("if-match: \"same\""));
            if ok {
                assert!(server
                    .request()
                    .head
                    .to_lowercase()
                    .contains("if-match: \"same\""));
            }
        }
    });
}
