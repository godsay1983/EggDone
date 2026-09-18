use super::*;
use crate::{
    lifecycle_sync::{prepare, Document},
    note_attachment_sync::{self, NoteAttachmentSyncDocument},
    purge::Terminal,
    recurrence_transport::tests::{Reply, Server},
};
use sha2::{Digest, Sha256};
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
const ASSET: &str = "123e4567-e89b-42d3-a456-426614174002";
const MAIN: &str = "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/todos.json";
fn sha() -> String {
    format!("{:x}", Sha256::digest(b"data"))
}
fn document() -> Document {
    Document {
        format_version: 1,
        terminals: vec![Terminal {
            kind: "note".into(),
            uuid: NOTE.into(),
            operation_uuid: ASSET.into(),
            purged_at: 100,
        }],
    }
}
fn assets() -> NoteAttachmentSyncDocument {
    serde_json::from_value(serde_json::json!({"format_version":1,"device_id":NOTE,"generated_at":100,"attachments":[{"uuid":ASSET,"note_uuid":NOTE,"kind":"file","display_name":"test.txt","mime_type":"text/plain","byte_size":4,"sha256":sha(),"preview_mime_type":null,"preview_byte_size":null,"preview_sha256":null,"width":null,"height":null,"sort_order":0,"created_at":1,"updated_at":100,"deleted_at":null,"updated_by":NOTE}]})).unwrap()
}
fn seed() -> (Connection, String) {
    seed_kind(false)
}
fn seed_kind(image: bool) -> (Connection, String) {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute("UPDATE sync_settings SET object_key=?1,enabled=1", [MAIN])
        .unwrap();
    let epoch = crate::sync_target::capture(&c).unwrap();
    let snapshot = prepare(&mut c, &epoch, &document()).unwrap();
    let mut incoming = assets();
    if image {
        let asset = &mut incoming.attachments[0];
        asset.kind = "image".into();
        asset.mime_type = "image/png".into();
        asset.preview_mime_type = Some("image/jpeg".into());
        asset.preview_byte_size = Some(4);
        asset.preview_sha256 = Some(sha());
        asset.width = Some(1);
        asset.height = Some(1);
    }
    assert!(
        note_attachment_sync::merge_remote_document(&mut c, &incoming, 100)
            .unwrap()
            .attachments
            .is_empty()
    );
    lifecycle_sync::acknowledge(&mut c, &epoch, snapshot.revision, "\"ledger\"").unwrap();
    (c, epoch)
}
#[test]
fn partial_image_cleanup_resumes_without_marking_failure_complete() {
    let (c, epoch) = seed_kind(true);
    let mut replies = reads("\"ledger\"", false);
    replies.extend([head(&sha()), Reply::new(204, None, b"")]);
    replies.extend(reads("\"ledger\"", false));
    replies.extend([head(&sha()), Reply::new(403, None, b"")]);
    replies.extend(reads("\"ledger\"", false));
    replies.push(Reply::new(404, None, b""));
    replies.extend(reads("\"ledger\"", false));
    replies.extend([head(&sha()), Reply::new(204, None, b"")]);
    let server = Server::new(replies);
    let prepared = PreparedManualSync::from_test_space(&c, server.bucket(), MAIN);
    let db = Database {
        connection: std::sync::Mutex::new(c),
    };
    assert!(tauri::async_runtime::block_on(run(
        &db,
        &SyncRuntime::default(),
        &prepared,
        "\"ledger\""
    ))
    .is_err());
    assert_eq!(
        pending(&db.connection.lock().unwrap(), &epoch)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        tauri::async_runtime::block_on(run(&db, &SyncRuntime::default(), &prepared, "\"ledger\""))
            .unwrap(),
        1
    );
    assert!(pending(&db.connection.lock().unwrap(), &epoch)
        .unwrap()
        .is_empty());
    let requests: Vec<_> = (0..15).map(|_| server.request()).collect();
    let deletes: Vec<_> = requests
        .iter()
        .filter(|r| r.head.starts_with("DELETE "))
        .collect();
    assert_eq!(deletes.len(), 3);
    assert!(deletes[0].head.contains("/original "));
    assert!(deletes[1].head.contains("/preview.jpg "));
    assert!(deletes[2].head.contains("/preview.jpg "));
}
fn reads(token: &str, referenced: bool) -> Vec<Reply> {
    let ledger = crate::sync_space::encode(
        &MAIN.replace("todos.json", "lifecycle-terminals.json"),
        &lifecycle_sync::encode(&document()).unwrap(),
    )
    .unwrap();
    let mut a = assets();
    if !referenced {
        a.attachments.clear();
    }
    let meta = crate::sync_space::encode(
        &MAIN.replace("todos.json", "note-attachments.json"),
        &serde_json::to_string(&a).unwrap(),
    )
    .unwrap();
    vec![
        Reply::new(200, Some(token), ledger.as_bytes()),
        Reply::new(200, Some("\"metadata\""), meta.as_bytes()),
    ]
}
fn head(hash: &str) -> Reply {
    Reply::new(200, Some("\"asset\""), b"data").with_header("x-amz-meta-sha256", hash)
}
#[test]
fn captures_remote_only_assets_and_refuses_conflicting_evidence_atomically() {
    let (mut c, epoch) = seed();
    assert_eq!(pending(&c, &epoch).unwrap().len(), 1);
    assert_eq!(
        c.query_row(
            "SELECT local_done FROM purge_cleanup WHERE attachment_uuid=?1",
            [ASSET],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let mut incoming = assets();
    incoming.attachments[0].sha256 = "b".repeat(64);
    let mut second = incoming.attachments[0].clone();
    second.uuid = "123e4567-e89b-42d3-a456-426614174000".into();
    incoming.attachments.insert(0, second);
    assert!(note_attachment_sync::merge_remote_document(&mut c, &incoming, 200).is_err());
    assert_eq!(pending(&c, &epoch).unwrap().len(), 1);
    let job = pending(&c, &epoch).unwrap().remove(0);
    acknowledge(&mut c, &job, "\"ledger\"").unwrap();
    assert!(pending(&c, &epoch).unwrap().is_empty());
    note_attachment_sync::merge_remote_document(&mut c, &assets(), 200).unwrap();
    assert_eq!(pending(&c, &epoch).unwrap().len(), 1);
    assert!(require_local(&c, &job, "\"stale\"").is_err());
    c.execute(
        "UPDATE app_metadata SET value='other' WHERE key='sync.target.epoch.v1'",
        [],
    )
    .unwrap();
    assert!(pending(&c, &epoch).is_err());
}
#[test]
fn production_cleanup_checks_ledger_references_hash_and_conditional_delete() {
    for scenario in [
        "success",
        "already-absent",
        "permission",
        "replaced",
        "hash-mismatch",
        "referenced",
        "ledger-change",
    ] {
        let (c, epoch) = seed();
        let hash = sha();
        let mut replies = reads(
            if scenario == "ledger-change" {
                "\"changed\""
            } else {
                "\"ledger\""
            },
            scenario == "referenced",
        );
        if scenario == "ledger-change" {
            replies.truncate(1);
        }
        if !["referenced", "ledger-change"].contains(&scenario) {
            replies.push(if scenario == "already-absent" {
                Reply::new(404, None, b"")
            } else {
                head(if scenario == "hash-mismatch" {
                    "bad"
                } else {
                    &hash
                })
            });
            if !["already-absent", "hash-mismatch"].contains(&scenario) {
                replies.push(Reply::new(
                    match scenario {
                        "permission" => 403,
                        "replaced" => 412,
                        _ => 204,
                    },
                    None,
                    b"",
                ));
            }
        }
        let server = Server::new(replies);
        let prepared = PreparedManualSync::from_test_space(&c, server.bucket(), MAIN);
        let db = Database {
            connection: std::sync::Mutex::new(c),
        };
        let result = tauri::async_runtime::block_on(run(
            &db,
            &SyncRuntime::default(),
            &prepared,
            "\"ledger\"",
        ));
        let success = ["success", "already-absent"].contains(&scenario);
        assert_eq!(result.is_ok(), success, "{scenario}: {result:?}");
        let c = db.connection.lock().unwrap();
        assert_eq!(
            pending(&c, &epoch).unwrap().len(),
            if success { 0 } else { 1 }
        );
        assert_eq!(
            c.query_row(
                "SELECT remote_done FROM purge_cleanup WHERE attachment_uuid=?1",
                [ASSET],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            if success { 1 } else { 0 }
        );
        if scenario == "success" {
            for _ in 0..3 {
                server.request();
            }
            let request = server.request();
            assert!(request.head.starts_with("DELETE "));
            assert!(request.head.to_lowercase().contains("if-match: \"asset\""));
        }
    }
}
#[test]
fn target_switch_during_head_prevents_delete() {
    let (c, _) = seed();
    let db = std::sync::Arc::new(Database {
        connection: std::sync::Mutex::new(c),
    });
    let other = db.clone();
    let mut replies = reads("\"ledger\"", false);
    replies.push(head(&sha()).with_hook(move || {
        other
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE app_metadata SET value='changed' WHERE key='sync.target.epoch.v1'",
                [],
            )
            .unwrap();
    }));
    let server = Server::new(replies);
    let prepared =
        PreparedManualSync::from_test_space(&db.connection.lock().unwrap(), server.bucket(), MAIN);
    assert!(tauri::async_runtime::block_on(run(
        &db,
        &SyncRuntime::default(),
        &prepared,
        "\"ledger\""
    ))
    .is_err());
    assert_eq!(
        db.connection
            .lock()
            .unwrap()
            .query_row("SELECT remote_done FROM purge_cleanup", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn same_target_resave_rebinds_pending_proof_but_other_targets_do_not() {
    let (mut c, old_epoch) = seed();
    let old = pending(&c, &old_epoch).unwrap().remove(0);
    crate::sync_target::invalidate(&c).unwrap();
    crate::sync_target::activate(&c).unwrap();
    let epoch = crate::sync_target::capture(&c).unwrap();
    let jobs = pending(&c, &epoch).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1, epoch);
    assert!(value(&c, &old.key(PREFIX)).unwrap().is_none());
    assert!(require_local(&c, &old, "\"ledger\"").is_err());
    assert!(require_local(&c, &jobs[0], "\"ledger\"").is_err());
    let state = prepare(&mut c, &epoch, &document()).unwrap();
    lifecycle_sync::acknowledge(&mut c, &epoch, state.revision, "\"new-ledger\"").unwrap();
    require_local(&c, &jobs[0], "\"new-ledger\"").unwrap();

    for field in ["endpoint", "bucket"] {
        crate::sync_target::invalidate(&c).unwrap();
        c.execute(
            &format!("UPDATE sync_settings SET {field}='other-target'"),
            [],
        )
        .unwrap();
        crate::sync_target::activate(&c).unwrap();
        let other_epoch = crate::sync_target::capture(&c).unwrap();
        assert!(pending(&c, &other_epoch).unwrap().is_empty());
        assert!(value(&c, &jobs[0].key(PREFIX)).unwrap().is_some());
        assert_eq!(
            c.query_row("SELECT remote_done FROM purge_cleanup", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn missing_target_binding_and_conflicting_rebind_fail_without_losing_old_evidence() {
    let (c, old_epoch) = seed();
    let old = pending(&c, &old_epoch).unwrap().remove(0);
    let binding = value(&c, &format!("{TARGET}{old_epoch}")).unwrap().unwrap();
    c.execute(
        "DELETE FROM app_metadata WHERE key=?1",
        [format!("{TARGET}{old_epoch}")],
    )
    .unwrap();
    crate::sync_target::invalidate(&c).unwrap();
    crate::sync_target::activate(&c).unwrap();
    let epoch = crate::sync_target::capture(&c).unwrap();
    assert_eq!(
        pending(&c, &epoch).unwrap_err(),
        "PURGE_REMOTE_TARGET_UNKNOWN"
    );
    assert!(value(&c, &old.key(PREFIX)).unwrap().is_some());
    c.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![format!("{TARGET}{old_epoch}"), binding],
    )
    .unwrap();
    let mut conflict = old.clone();
    conflict.1 = epoch.clone();
    conflict.7 = "b".repeat(64);
    save(&c, &conflict).unwrap();
    assert_eq!(
        pending(&c, &epoch).unwrap_err(),
        "PURGE_REMOTE_EVIDENCE_CONFLICT"
    );
    assert!(value(&c, &old.key(PREFIX)).unwrap().is_some());
    assert_eq!(
        value(&c, &conflict.key(PREFIX)).unwrap().unwrap(),
        conflict.raw().unwrap()
    );
}

#[test]
fn uppercase_attachment_identity_preserves_its_exact_storage_path() {
    let (mut c, epoch) = seed();
    let mut remote = assets();
    remote.attachments[0].uuid = ASSET.to_uppercase();
    assert!(
        note_attachment_sync::merge_remote_document(&mut c, &remote, 100)
            .unwrap()
            .attachments
            .is_empty()
    );
    assert!(pending(&c, &epoch)
        .unwrap()
        .iter()
        .any(|job| job.4 == ASSET.to_uppercase()));
}
