//! Isolated production core and cleanup worker, not activation/UI acceptance.
use super::*;
const SPACE: &str = "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/";
fn prepared(client: &Client) -> s3_sync::PreparedManualSync {
    let c = client.db.connection.lock().unwrap();
    c.execute(
        "UPDATE sync_settings SET object_key=?1",
        [format!("{SPACE}todos.json")],
    )
    .unwrap();
    s3_sync::PreparedManualSync::from_test_space(&c, bucket(SECRET), &format!("{SPACE}todos.json"))
}
#[test]
#[ignore = "Use run-sync-core-s3.ps1 -PurgeRemoteSessions"]
fn prepare() {
    tauri::async_runtime::block_on(async {
        let client = Client::new(TODO, NOTE);
        let p = prepared(&client);
        let old = bucket(SECRET)
            .get_object("account/notes.json")
            .await
            .unwrap()
            .to_vec();
        let ledger = crate::lifecycle_sync::Document {
            format_version: 1,
            terminals: vec![crate::purge::Terminal {
                kind: "note".into(),
                uuid: NOTE.into(),
                operation_uuid: ASSET.into(),
                purged_at: 1000,
            }],
        };
        let key = format!("{SPACE}lifecycle-terminals.json");
        let body =
            crate::sync_space::encode(&key, &crate::lifecycle_sync::encode(&ledger).unwrap())
                .unwrap();
        assert_eq!(
            bucket(SECRET)
                .put_object(&key, body.as_bytes())
                .await
                .unwrap()
                .status_code(),
            200
        );
        let hash = format!("{:x}", sha2::Sha256::digest(BYTES));
        let metadata = s3_sync::download_note_attachment_remote(&p)
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(metadata.attachments[0].sha256, hash);
        s3_sync::upload_immutable_asset(
            &client.runtime,
            &p,
            ASSET,
            "original",
            BYTES,
            "text/markdown",
            &hash,
        )
        .await
        .unwrap();
        let _lock = client.runtime.acquire().unwrap();
        sync_now_inner(&client.db, &client.runtime, &client.assets, &p, || {})
            .await
            .unwrap();
        assert!(
            !s3_sync::head_asset_object(&p, ASSET, "original")
                .await
                .unwrap()
                .exists
        );
        assert!(s3_sync::download_note_attachment_remote(&p)
            .await
            .unwrap()
            .document
            .unwrap()
            .attachments
            .is_empty());
        assert!(s3_sync::download_note_remote(&p)
            .await
            .unwrap()
            .document
            .unwrap()
            .notes
            .iter()
            .all(|n| n.uuid != NOTE));
        assert_eq!(
            bucket(SECRET)
                .get_object("account/notes.json")
                .await
                .unwrap()
                .as_slice(),
            old
        );
        assert_eq!(
            client
                .db
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT remote_done FROM purge_cleanup WHERE attachment_uuid=?1",
                    [ASSET],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        println!("PURGE_REMOTE_DESKTOP_PREPARE_OK: full production core applies terminal, filters remote-only attachment and deletes verified binary; old space retained");
    });
}
#[test]
#[ignore = "Requires Harmony cleanup and stale-note fixture"]
fn verify() {
    tauri::async_runtime::block_on(async {
        let client = Client::new(TODO, NOTE);
        let p = prepared(&client);
        let _lock = client.runtime.acquire().unwrap();
        sync_now_inner(&client.db, &client.runtime, &client.assets, &p, || {})
            .await
            .unwrap();
        let c = client.db.connection.lock().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM notes WHERE uuid IN (?1,?2)",
                params![NOTE, NOTE2],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(
            c.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
                .unwrap()
                > 0
        );
        println!("PURGE_REMOTE_DESKTOP_VERIFY_OK: fresh stale desktop keeps tasks and cannot resurrect either note");
    });
}
