//! Opt-in desktop/native exchange against the script-owned loopback S3 server.
use super::*;
use crate::{task_note_link_protocol as protocol, task_note_link_store as store};
use rusqlite::Connection;
use s3::{creds::Credentials, region::Region, BucketConfiguration};

const ACCESS: &str = "eggdone-ns7-test-access";
const SECRET: &str = "eggdone-ns7-public-test-fixture";
const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
const NOTE2: &str = "123e4567-e89b-42d3-a456-426614174002";

fn target(secret: &str) -> (Box<Bucket>, String) {
    let run = std::env::var("EGGDONE_NS7_S3_RUN").expect("Use the isolated integration script");
    assert_eq!(run.len(), 32);
    assert!(run.bytes().all(|c| c.is_ascii_hexdigit()));
    let port = std::env::var("EGGDONE_NS7_S3_PORT")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    assert!(port >= 1024);
    let bucket = Bucket::new(
        &format!("eggdone-ns7-{run}"),
        Region::Custom {
            region: "us-east-1".into(),
            endpoint: format!("http://127.0.0.1:{port}"),
        },
        Credentials::new(Some(ACCESS), Some(secret), None, None, None).unwrap(),
    )
    .unwrap()
    .with_path_style();
    (bucket, format!("{run}/同步/%2F/todos.json"))
}

fn link(note: &str, stamp: i64, by: &str, deleted: bool) -> protocol::TaskNoteLink {
    protocol::TaskNoteLink {
        uuid: protocol::link_uuid(TODO, note).unwrap(),
        todo_uuid: TODO.into(),
        note_uuid: note.into(),
        created_at: 10,
        updated_at: stamp,
        updated_by: by.into(),
        deleted_at: if deleted { Some(stamp) } else { None },
    }
}

fn document() -> LinkDocument {
    LinkDocument {
        format_version: 1,
        links: vec![link(NOTE, 100, "l3d-desktop", false)],
    }
}

fn database() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}

#[test]
#[ignore = "Use run-s3-integration.ps1 -TaskNoteLinks; never a production bucket"]
fn l3d_s3_prepare() {
    tauri::async_runtime::block_on(async {
        let (bucket, key) = target(SECRET);
        let created = Bucket::create_with_path_style(
            &bucket.name,
            bucket.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .unwrap();
        assert_eq!(created.response_code, 200);
        let transport = TaskNoteLinkTransport::new(&bucket, &key, &[]).unwrap();
        assert_eq!(transport.probe().await.unwrap(), "missing");
        let absent = transport.download().await.unwrap();
        assert!(absent.document.is_none());
        let mut db = database();
        store::merge(&mut db, &document()).unwrap();
        let snapshot = store::snapshot(&db).unwrap();
        assert!(matches!(
            transport.upload(&snapshot.document, &absent).await.unwrap(),
            LinkUploadOutcome::Uploaded { .. }
        ));
        assert_eq!(
            transport.upload(&document(), &absent).await.unwrap(),
            LinkUploadOutcome::Conflict
        );
        assert_eq!(
            transport.download().await.unwrap().document,
            Some(document())
        );
        let (bad_bucket, _) = target("intentionally-invalid-fixture");
        let bad = TaskNoteLinkTransport::new(&bad_bucket, &key, &[]).unwrap();
        assert_eq!(bad.probe().await.unwrap(), "denied");
        assert_eq!(
            bad.download().await.err().unwrap(),
            "TASK_NOTE_LINK_DOWNLOAD_HTTP:403"
        );
        // A rejected signature must not affect local state or prevent a subsequent valid read.
        let after = store::snapshot(&db).unwrap();
        assert_eq!(after.document, snapshot.document);
        assert_eq!(after.revision, snapshot.revision);
        assert_eq!(after.synced_revision, snapshot.synced_revision);
        assert_eq!(after.etag, snapshot.etag);
        assert_eq!(
            transport.download().await.unwrap().document,
            Some(document())
        );
        println!("L3D_S3_DESKTOP_PREPARE_OK");
    });
}

#[test]
#[ignore = "Requires the native peer to merge an offline link and unlink the desktop link first"]
fn l3d_s3_verify() {
    tauri::async_runtime::block_on(async {
        let (bucket, key) = target(SECRET);
        let transport = TaskNoteLinkTransport::new(&bucket, &key, &[]).unwrap();
        let remote = transport.download().await.unwrap();
        let expected = protocol::merge_documents(
            &LinkDocument {
                format_version: 1,
                links: vec![
                    link(NOTE, 300, "l3d-harmony", true),
                    link(NOTE2, 150, "l3d-harmony", false),
                ],
            },
            &LinkDocument {
                format_version: 1,
                links: vec![],
            },
        )
        .unwrap();
        assert_eq!(remote.document.as_ref(), Some(&expected));
        let mut db = database();
        // A desktop-side offline active edit loses to the later native unlink.
        let offline = LinkDocument {
            format_version: 1,
            links: vec![link(NOTE, 200, "l3d-desktop", false)],
        };
        store::merge(&mut db, &offline).unwrap();
        store::merge(&mut db, remote.document.as_ref().unwrap()).unwrap();
        assert_eq!(store::snapshot(&db).unwrap().document, expected);
        let mut backup = document();
        backup.links[0].updated_at = 5000;
        let tx = db.transaction().unwrap();
        crate::task_note_link_backup::restore(&tx, Some(&backup), 6000).unwrap();
        tx.commit().unwrap();
        assert_eq!(store::snapshot(&db).unwrap().document, expected);
        let before = transport.probe().await.unwrap();
        let mut final_doc = expected.clone();
        let active = final_doc
            .links
            .iter_mut()
            .find(|l| l.note_uuid == NOTE2)
            .unwrap();
        active.updated_at = 400;
        active.updated_by = "l3d-desktop-final".into();
        assert!(matches!(
            transport.upload(&final_doc, &remote).await.unwrap(),
            LinkUploadOutcome::Uploaded { .. }
        ));
        assert_ne!(transport.probe().await.unwrap(), before);
        assert_eq!(
            transport.upload(&offline, &remote).await.unwrap(),
            LinkUploadOutcome::Conflict
        );
        let downloaded = transport.download().await.unwrap().document.unwrap();
        store::merge(&mut db, &downloaded).unwrap();
        assert_eq!(store::snapshot(&db).unwrap().document, final_doc);
        assert_eq!(downloaded, final_doc);
        println!("L3D_S3_DESKTOP_VERIFY_OK");
    });
}
