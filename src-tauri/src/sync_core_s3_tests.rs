//! Opt-in real-S3 integration for the complete desktop synchronization core.
use super::*;
use s3::{creds::Credentials, region::Region, BucketConfiguration};

const ACCESS: &str = "eggdone-ns7-test-access";
const SECRET: &str = "eggdone-ns7-public-test-fixture";

#[path = "sync_core_history_s3_tests.rs"]
mod history;

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -TrashRecoverySessions with the Harmony peer"]
fn trash_session_prepare() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let response = Bucket::create_with_path_style(
            &target.name,
            target.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .unwrap();
        assert_eq!(response.response_code, 200);
        let desktop = Client::new(TODO, NOTE);
        desktop.add_file();
        {
            let db = desktop.db.connection.lock().unwrap();
            db.execute("UPDATE todos SET completed=1,due_date='2026-09-13',reminder_at=9999999999999,repeat_rule='daily',repeat_next_due_date='2026-09-14' WHERE uuid=?1", [TODO]).unwrap();
        }
        desktop.sync(bucket(SECRET)).await.unwrap();
        let second = Client::new(TODO2, NOTE2);
        second.sync(bucket(SECRET)).await.unwrap();
        // Seed deletion state; the production sync core and later restore APIs are under test.
        {
            let db = desktop.db.connection.lock().unwrap();
            let stamp = now_millis() + 1000;
            db.execute(
                "UPDATE todos SET deleted_at=?1,updated_at=?1 WHERE uuid=?2",
                params![stamp, TODO],
            )
            .unwrap();
            db.execute(
                "UPDATE notes SET deleted_at=?1,updated_at=?1 WHERE uuid=?2",
                params![stamp, NOTE],
            )
            .unwrap();
        }
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert!(desktop.state().dirty_domains.is_empty());
        assert_eq!(
            crate::trash::list(&desktop.db.connection.lock().unwrap(), 0, 50)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            target
                .get_object(&format!("account/assets/{ASSET}/original"))
                .await
                .unwrap()
                .as_slice(),
            BYTES
        );
        println!("TRASH_SESSION_DESKTOP_PREPARE_OK");
    });
}

#[test]
#[ignore = "Requires Harmony trash exchange; never use a user bucket"]
fn trash_session_verify() {
    tauri::async_runtime::block_on(async {
        // A stale active peer was offline throughout deletion and restoration.
        let desktop = Client::new(TODO, NOTE);
        desktop.sync(bucket(SECRET)).await.unwrap();
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let task: (bool, Option<i64>, Option<String>, Option<i64>) = db
                .query_row(
                    "SELECT completed,reminder_at,repeat_rule,deleted_at FROM todos WHERE uuid=?1",
                    [TODO],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap();
            assert_eq!(task, (true, None, None, None));
            assert_eq!(crate::trash::list(&db, 0, 50).unwrap().len(), 2);
            let by = crate::db::device_id(&db).unwrap();
            for (kind, id) in [
                (crate::trash::TrashKind::Todo, TODO2),
                (crate::trash::TrashKind::Note, NOTE2),
            ] {
                let preview = crate::trash::preview(&db, kind, id).unwrap();
                crate::trash::restore(&mut db, &preview, now_millis(), &by).unwrap();
            }
        }
        desktop.sync(bucket(SECRET)).await.unwrap();
        // A different offline peer carries an obsolete deletion instead of an active copy.
        let stale = Client::new(TODO, NOTE);
        stale.db.connection.lock().unwrap().execute_batch(
            "UPDATE todos SET updated_at=500,deleted_at=500; UPDATE notes SET updated_at=500,deleted_at=500;"
        ).unwrap();
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();
        for client in [&desktop, &stale] {
            let db = client.db.connection.lock().unwrap();
            assert!(crate::trash::list(&db, 0, 50).unwrap().is_empty());
            let snapshot = links::snapshot(&db).unwrap();
            assert_eq!(snapshot.document.links.len(), 2);
            assert!(snapshot
                .document
                .links
                .iter()
                .all(|link| link.deleted_at.is_some()));
            assert_eq!(snapshot.revision, snapshot.synced_revision);
            drop(db);
            assert!(client.state().dirty_domains.is_empty());
        }
        let attachment =
            note_attachments::list_active_by_note(&desktop.db.connection.lock().unwrap(), NOTE)
                .unwrap()
                .remove(0);
        assert!(attachment.local_original_path.is_none());
        assert_eq!(attachment.transfer_state, "remote_only");
        let prepared = s3_sync::PreparedManualSync::from_test_bucket(
            &desktop.db.connection.lock().unwrap(),
            bucket(SECRET),
        );
        let bytes = s3_sync::download_asset_bytes(
            &desktop.runtime,
            &prepared,
            ASSET,
            "original",
            attachment.byte_size,
            &attachment.sha256,
        )
        .await
        .unwrap();
        assert_eq!(bytes, BYTES);
        let response = bucket(SECRET)
            .delete_object(&format!("account/assets/{ASSET}/original"))
            .await
            .unwrap();
        assert!((200..300).contains(&response.status_code()));
        assert!(s3_sync::download_asset_bytes(
            &desktop.runtime,
            &prepared,
            ASSET,
            "original",
            attachment.byte_size,
            &attachment.sha256
        )
        .await
        .is_err());
        assert_eq!(
            note_attachments::list_active_by_note(&desktop.db.connection.lock().unwrap(), NOTE)
                .unwrap()
                .len(),
            1
        );
        println!("TRASH_SESSION_DESKTOP_VERIFY_OK: stale active/deleted peers, reverse restore, no old links, original hash and missing bytes");
    });
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -CrossClientSessions with the Harmony peer"]
fn full_session_prepare() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let response = Bucket::create_with_path_style(
            &target.name,
            target.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .unwrap();
        assert_eq!(response.response_code, 200);
        let desktop = Client::new(TODO, NOTE);
        desktop.add_file();
        let result = desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(
            (
                result.todo_count,
                result.note_count,
                result.note_attachment_count
            ),
            (1, 1, 1)
        );
        assert!(desktop.state().dirty_domains.is_empty());
        println!("FULL_SESSION_DESKTOP_PREPARE_OK");
    });
}

#[test]
#[ignore = "Requires the Harmony full SyncService exchange phase first"]
fn full_session_verify() {
    tauri::async_runtime::block_on(async {
        let desktop = Client::new(TODO, NOTE);
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let mut doc = links::snapshot(&db).unwrap().document;
            doc.links[0].updated_at = 200;
            links::merge(&mut db, &doc).unwrap();
        }
        let result = desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(
            (
                result.todo_count,
                result.note_count,
                result.note_attachment_count
            ),
            (2, 2, 1)
        );
        assert!(desktop.state().dirty_domains.is_empty());
        let snapshot = links::snapshot(&desktop.db.connection.lock().unwrap()).unwrap();
        assert_eq!(snapshot.revision, snapshot.synced_revision);
        assert_eq!(snapshot.document.links.len(), 2);
        assert_eq!(
            snapshot
                .document
                .links
                .iter()
                .find(|l| l.todo_uuid == TODO)
                .unwrap()
                .deleted_at,
            Some(300)
        );
        assert!(snapshot
            .document
            .links
            .iter()
            .find(|l| l.todo_uuid == TODO2)
            .unwrap()
            .deleted_at
            .is_none());
        {
            let db = desktop.db.connection.lock().unwrap();
            assert_eq!(
                db.query_row("SELECT content FROM notes WHERE uuid=?1", [NOTE], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
                "harmony note revision"
            );
            assert_eq!(
                db.query_row("SELECT title FROM todos WHERE uuid=?1", [TODO2], |r| r
                    .get::<_, String>(
                    0
                ))
                .unwrap(),
                "harmony changed task"
            );
            assert_eq!(
                db.query_row(
                    "SELECT display_name FROM note_attachments WHERE uuid=?1",
                    [ASSET],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "harmony renamed fixture.md"
            );
            db.execute(
                "UPDATE todos SET title='desktop final title',updated_at=400 WHERE uuid=?1",
                [TODO],
            )
            .unwrap();
        }
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert!(desktop.state().dirty_domains.is_empty());
        println!("FULL_SESSION_DESKTOP_VERIFY_OK");
    });
}

fn bucket(secret: &str) -> Box<Bucket> {
    let run = std::env::var("EGGDONE_NS7_S3_RUN").expect("Use scripts/run-sync-core-s3.ps1");
    assert_eq!(run.len(), 32);
    assert!(run
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
    let port: u16 = std::env::var("EGGDONE_NS7_S3_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert!(port >= 1024);
    Bucket::new(
        &format!("eggdone-ns7-{run}"),
        Region::Custom {
            region: "us-east-1".into(),
            endpoint: format!("http://127.0.0.1:{port}"),
        },
        Credentials::new(Some(ACCESS), Some(secret), None, None, None).unwrap(),
    )
    .unwrap()
    .with_path_style()
}

#[test]
#[ignore = "Requires scripts/run-sync-core-s3.ps1; never a user bucket"]
fn isolated_s3_offline_peers_unlink_binary_and_credentials_recovery() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let response = Bucket::create_with_path_style(
            &target.name,
            target.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .unwrap();
        assert_eq!(response.response_code, 200);

        // Both clients create different entities while offline, before either downloads anything.
        let desktop = Client::new(TODO, NOTE);
        let peer = Client::new(TODO2, NOTE2);
        desktop.add_file();
        assert!(desktop.state().dirty_domains.contains(&"links".into()));
        assert!(peer.state().dirty_domains.contains(&"links".into()));
        let first = desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(
            (
                first.todo_count,
                first.note_count,
                first.note_attachment_count
            ),
            (1, 1, 1)
        );
        assert!(first.link_remote_token.unwrap().starts_with("etag:"));
        assert!(desktop.state().dirty_domains.is_empty());
        assert_eq!(
            target
                .get_object(&format!("account/assets/{ASSET}/original"))
                .await
                .unwrap()
                .as_slice(),
            BYTES
        );

        let second = peer.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(
            (
                second.todo_count,
                second.note_count,
                second.note_attachment_count
            ),
            (2, 2, 1)
        );
        assert!(peer.state().dirty_domains.is_empty());
        let remote_file =
            note_attachments::list_active_by_note(&peer.db.connection.lock().unwrap(), NOTE)
                .unwrap()
                .remove(0);
        assert!(remote_file.remote_uploaded);
        assert_eq!(remote_file.transfer_state, "remote_only");
        assert!(remote_file.local_original_path.is_none());
        let prepared = s3_sync::PreparedManualSync::from_test_bucket(
            &peer.db.connection.lock().unwrap(),
            bucket(SECRET),
        );
        let downloaded = s3_sync::download_asset_bytes(
            &peer.runtime,
            &prepared,
            ASSET,
            "original",
            remote_file.byte_size,
            &remote_file.sha256,
        )
        .await
        .unwrap();
        assert_eq!(downloaded, BYTES);

        // One peer unlinks while the original peer still has an older active offline edit.
        {
            let mut db = peer.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            let mut document = links::snapshot(&db).unwrap().document;
            let link = document
                .links
                .iter_mut()
                .find(|link| link.todo_uuid == TODO)
                .unwrap();
            link.updated_at = 300;
            link.deleted_at = Some(300);
            link.updated_by = by.clone();
            links::merge(&mut db, &document).unwrap();
            db.execute("UPDATE notes SET content='peer revision',updated_at=300,updated_by=?2 WHERE uuid=?1", params![NOTE, by]).unwrap();
        }
        peer.sync(bucket(SECRET)).await.unwrap();
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let mut document = links::snapshot(&db).unwrap().document;
            document.links[0].updated_at = 200;
            links::merge(&mut db, &document).unwrap();
            db.execute(
                "UPDATE todos SET title='offline title',updated_at=200 WHERE uuid=?1",
                [TODO],
            )
            .unwrap();
        }
        let before = desktop.state();
        let link_before = links::snapshot(&desktop.db.connection.lock().unwrap()).unwrap();
        let notifications = desktop.notifications.load(Ordering::SeqCst);
        assert!(desktop
            .sync(bucket("intentionally-invalid-fixture"))
            .await
            .is_err());
        assert_eq!(desktop.state().dirty_domains, before.dirty_domains);
        let after = links::snapshot(&desktop.db.connection.lock().unwrap()).unwrap();
        assert_eq!(after.document, link_before.document);
        assert_eq!(after.synced_revision, link_before.synced_revision);
        assert_eq!(desktop.notifications.load(Ordering::SeqCst), notifications);
        assert!(desktop.runtime.acquire().is_ok());

        let recovered = desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(
            (
                recovered.todo_count,
                recovered.note_count,
                recovered.note_attachment_count
            ),
            (2, 2, 1)
        );
        assert!(desktop.state().dirty_domains.is_empty());
        peer.sync(bucket(SECRET)).await.unwrap();
        assert!(peer.state().dirty_domains.is_empty());
        let a = links::snapshot(&desktop.db.connection.lock().unwrap()).unwrap();
        let b = links::snapshot(&peer.db.connection.lock().unwrap()).unwrap();
        assert_eq!(a.document, b.document);
        assert_eq!(a.revision, a.synced_revision);
        assert_eq!(b.revision, b.synced_revision);
        assert_eq!(a.document.links.len(), 2);
        assert_eq!(
            a.document
                .links
                .iter()
                .find(|l| l.todo_uuid == TODO)
                .unwrap()
                .deleted_at,
            Some(300)
        );
        assert!(a
            .document
            .links
            .iter()
            .find(|l| l.todo_uuid == TODO2)
            .unwrap()
            .deleted_at
            .is_none());
        for client in [&desktop, &peer] {
            let db = client.db.connection.lock().unwrap();
            assert_eq!(
                db.query_row("SELECT content FROM notes WHERE uuid=?1", [NOTE], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
                "peer revision"
            );
            assert_eq!(
                db.query_row("SELECT title FROM todos WHERE uuid=?1", [TODO], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "offline title"
            );
        }
        println!("SYNC_CORE_S3_DESKTOP_OK: offline peers, entities, unlink tombstone, binary, dirty/ACK, rejected credentials, recovery");
    });
}
