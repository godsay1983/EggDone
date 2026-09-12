//! Opt-in real-S3 integration for the complete desktop synchronization core.
use super::*;
use s3::{creds::Credentials, region::Region, BucketConfiguration};

const ACCESS: &str = "eggdone-ns7-test-access";
const SECRET: &str = "eggdone-ns7-public-test-fixture";

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
