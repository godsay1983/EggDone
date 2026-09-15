use super::*;
use crate::{sync_runtime_state, task_template_backup as backup, task_template_store as store};
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn document() -> protocol::TemplatesDocument {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/task-templates-v1.json")).unwrap();
    protocol::parse(
        &serde_json::json!({"format_version":1,"templates":[f["base"].clone()]}).to_string(),
    )
    .unwrap()
}
#[test]
fn template_ack_tracks_exact_snapshot_target_and_missing_proof() {
    let mut db = db();
    let epoch = sync_target::capture(&db).unwrap();
    let empty = protocol::encode(&protocol::TemplatesDocument::default()).unwrap();
    let s = prepare(&mut db, &epoch, &empty).unwrap();
    assert!(acknowledge(&mut db, &s, None).unwrap());
    let s = prepare(&mut db, &epoch, &protocol::encode(&document()).unwrap()).unwrap();
    assert!(sync_runtime_state::get_snapshot(&db)
        .unwrap()
        .dirty_domains
        .contains(&"templates".into()));
    assert!(acknowledge(&mut db, &s, None).is_err());
    assert!(acknowledge(&mut db, &s, Some("W/\"weak\"")).is_err());
    assert!(acknowledge(&mut db, &s, Some("\"one\"")).unwrap());
    assert!(!sync_runtime_state::get_snapshot(&db)
        .unwrap()
        .dirty_domains
        .contains(&"templates".into()));
    let mut changed = document();
    changed.templates[0].updated_at += 1;
    changed.templates[0].content.name = "Changed".into();
    store::merge(&mut db, &changed).unwrap();
    assert!(!acknowledge(&mut db, &s, Some("\"late\"")).unwrap());
    let current = prepare(&mut db, &epoch, &empty).unwrap();
    sync_target::invalidate(&db).unwrap();
    sync_target::activate(&db).unwrap();
    assert!(!acknowledge(&mut db, &current, Some("\"old-target\"")).unwrap());
    assert!(prepare(&mut db, &epoch, &empty).is_err());
    let state = store::snapshot(&mut db).unwrap();
    assert!(state.revision > state.synced_revision);
    assert_eq!(state.etag, None);
}
#[test]
fn template_restore_retains_legacy_tombstones_and_rolls_back_ack_failure() {
    let mut db = db();
    let doc = document();
    store::merge(&mut db, &doc).unwrap();
    let before = store::snapshot(&mut db).unwrap();
    {
        let tx = db.transaction().unwrap();
        backup::restore(&tx, None).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(before, store::snapshot(&mut db).unwrap());
    for version in 1..=4 {
        backup::validate(version, None).unwrap();
        assert!(backup::validate(version, Some(&doc)).is_err());
    }
    assert!(backup::validate(5, None).is_err());
    backup::validate(5, Some(&doc)).unwrap();
    let mut deleted = doc.clone();
    let row = &mut deleted.templates[0];
    row.updated_at += 1;
    row.deleted_at = Some(row.updated_at);
    store::merge(&mut db, &deleted).unwrap();
    {
        let tx = db.transaction().unwrap();
        backup::restore(&tx, Some(&doc)).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(store::snapshot(&mut db).unwrap().document, deleted);
    let epoch = sync_target::capture(&db).unwrap();
    let s = prepare(&mut db, &epoch, &protocol::encode(&doc).unwrap()).unwrap();
    db.execute_batch("CREATE TRIGGER fail_template_ack BEFORE UPDATE OF synced_revision ON task_template_sync_state BEGIN SELECT RAISE(ABORT,'ack'); END;").unwrap();
    let before = store::snapshot(&mut db).unwrap();
    assert!(acknowledge(&mut db, &s, Some("\"etag\"")).is_err());
    assert_eq!(before, store::snapshot(&mut db).unwrap());
    {
        let tx = db.transaction().unwrap();
        assert!(backup::restore(&tx, Some(&doc)).is_err());
    }
    assert_eq!(before, store::snapshot(&mut db).unwrap());
}
#[test]
fn template_key_is_independent_and_collision_checked() {
    assert_eq!(
        object_key("account/todos.json", &[]).unwrap(),
        "account/task-templates.json"
    );
    assert!(object_key("account/task-templates.json", &[]).is_err());
    assert!(object_key(
        "account/todos.json",
        &["account/task-templates.json".into()]
    )
    .is_err());
    assert!(object_key("../todos.json", &[]).is_err());
}

#[test]
#[ignore = "run only with the isolated Harmony template HTTP harness"]
fn template_loopback_peer() {
    use s3::{creds::Credentials, Bucket, Region};
    use std::sync::Mutex;
    let run = std::env::var("EGGDONE_NS7_S3_RUN").unwrap();
    assert_eq!(run.len(), 32);
    assert!(run.bytes().all(|c| c.is_ascii_hexdigit()));
    let port: u16 = std::env::var("EGGDONE_NS7_S3_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert!(port >= 1024);
    let phase = std::env::var("EGGDONE_TEMPLATE_PHASE").unwrap();
    assert!(["seed", "verify"].contains(&phase.as_str()));
    let bucket = Bucket::new(
        &format!("eggdone-ns7-{run}"),
        Region::Custom {
            region: "us-east-1".into(),
            endpoint: format!("http://127.0.0.1:{port}"),
        },
        Credentials::new(
            Some("eggdone-ns7-test-access"),
            Some("eggdone-ns7-public-test-fixture"),
            None,
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap()
    .with_path_style();
    let mut connection = db();
    if phase == "seed" {
        store::merge(&mut connection, &document()).unwrap();
    }
    let db = crate::db::Database {
        connection: Mutex::new(connection),
    };
    let prepared = crate::s3_sync::PreparedManualSync::from_test_bucket(
        &db.connection.lock().unwrap(),
        bucket,
    );
    let result =
        tauri::async_runtime::block_on(crate::task_note_link_session::run(&db, &prepared)).unwrap();
    assert!(result.template_token.is_some());
    let s = store::snapshot(&mut db.connection.lock().unwrap()).unwrap();
    assert_eq!(s.revision, s.synced_revision);
    if phase == "verify" {
        assert_eq!(s.document.templates[0].content.name, "Edited on Harmony");
    }
    println!("TEMPLATE_DESKTOP_{}_OK", phase.to_uppercase());
}
