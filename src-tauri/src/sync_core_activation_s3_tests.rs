//! Real migration admission and terminal-aware sessions. Only credentials/filesystem are fixtures.
use super::*;
use crate::{
    migration_backup::{self as backup, cloud, publication},
    space_activation as space,
};
use std::cell::RefCell;
const MAIN: &str = "account/todos.json";
const MISSING: &str = "123e4567-e89b-42d3-a456-426614174099";

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -DirectPurgeSessions"]
fn direct_verify_and_purge() {
    let client = Client::new(TODO, NOTE);
    configure(&client.db.connection.lock().unwrap());
    // A stale body with a newer timestamp must lose to the peer's terminal.
    client
        .db
        .connection
        .lock()
        .unwrap()
        .execute("UPDATE notes SET updated_at=9007199254740991", [])
        .unwrap();
    sync(&client);
    let mut c = client.db.connection.lock().unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM notes WHERE uuid=?1", [NOTE], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let now = crate::db::now_millis();
    c.execute("UPDATE todos SET deleted_at=?1,updated_at=?1,repeat_rule=NULL,repeat_series_uuid=NULL WHERE uuid=?2", params![now, TODO]).unwrap();
    c.execute(
        "DELETE FROM recurrence_rules WHERE current_todo_uuid=?1",
        [TODO],
    )
    .unwrap();
    let plan = crate::purge::prepare(&mut c, None, now + 1).unwrap();
    assert_eq!(
        crate::purge::execute_batch(&mut c, &plan.operation_uuid, now + 2)
            .unwrap()
            .purged,
        1
    );
    assert!(
        crate::purge::status(&c, &plan.operation_uuid)
            .unwrap()
            .sync_pending
    );
    drop(c);
    sync(&client);
    let c = client.db.connection.lock().unwrap();
    assert!(
        !crate::purge::status(&c, &plan.operation_uuid)
            .unwrap()
            .sync_pending
    );
    assert_eq!(
        c.query_row("SELECT object_key FROM sync_settings", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        MAIN
    );
    assert!(!space::is_active(&c).unwrap());
    println!("DIRECT_PURGE_DESKTOP_OK: stale peer filtered, direct empty trash and terminal sync without activation");
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -ActivationSessions; isolated bucket only"]
fn missing_seed() {
    tauri::async_runtime::block_on(async {
        let b = bucket(SECRET);
        let key = s3_sync::derive_note_attachment_object_key(MAIN);
        let response = b.get_object(&key).await.unwrap();
        let mut doc: serde_json::Value = serde_json::from_slice(response.as_slice()).unwrap();
        let mut missing = doc["attachments"][0].clone();
        missing["uuid"] = MISSING.into();
        missing["deleted_at"] = 2.into();
        doc["attachments"].as_array_mut().unwrap().push(missing);
        assert_eq!(
            b.put_object(&key, &serde_json::to_vec(&doc).unwrap())
                .await
                .unwrap()
                .status_code(),
            200
        );
    });
    println!("ACTIVATION_MISSING_SEED_OK: deleted descriptor with no binary in isolated bucket");
}
fn sync(client: &Client) {
    let _guard = client.runtime.acquire().unwrap();
    let p = {
        let c = client.db.connection.lock().unwrap();
        let p = s3_sync::prepare_with_fixture_credentials(&c, bucket(SECRET)).unwrap();
        sync_runtime_state::begin_attempt(&c).unwrap();
        p
    };
    tauri::async_runtime::block_on(sync_now_inner(
        &client.db,
        &client.runtime,
        &client.assets,
        &p,
        || {},
    ))
    .unwrap();
    let c = client.db.connection.lock().unwrap();
    p.require_current(&c).unwrap();
    sync_runtime_state::record_success(&c).unwrap();
}
fn configure(c: &Connection) {
    let b = bucket(SECRET);
    let port = std::env::var("EGGDONE_NS7_S3_PORT").unwrap();
    c.execute("UPDATE sync_settings SET enabled=1,endpoint=?1,bucket=?2,region='us-east-1',object_key=?3,path_style=1,allow_http=1",params![format!("http://127.0.0.1:{port}"),b.name,MAIN]).unwrap();
}
fn local_backup(c: &mut Connection, root: &std::path::Path) -> backup::BackupPlan {
    let work = backup::prepare(c, crate::db::now_millis()).unwrap();
    backup::copy(&work, &root.join("copies"), &root.join("absent-assets")).unwrap();
    backup::rehearse(&root.join("copies"), &work.plan).unwrap();
    backup::finish(c, &work.plan, crate::db::now_millis()).unwrap();
    backup::latest(c).unwrap().unwrap()
}
#[test]
#[ignore = "Use run-sync-core-s3.ps1 -ActivationSessions"]
fn prepare() {
    let client = Client::new(TODO, NOTE);
    configure(&client.db.connection.lock().unwrap());
    sync(&client);
    let source = s3_sync::MigrationAssetSource::from_test_bucket(bucket(SECRET), MAIN);
    let remote = tauri::async_runtime::block_on(source.metadata()).unwrap();
    // Local remote-only assets are backed up through the real immutable downloader.
    let mut c = client.db.connection.lock().unwrap();
    let preflight = crate::migration_preflight::read(&mut c).unwrap();
    assert!(
        preflight.blockers().is_empty(),
        "migration preflight: {preflight:?}"
    );
    let mut work = backup::prepare(&mut c, crate::db::now_millis()).unwrap();
    let before = work.plan.clone();
    let copies = client.root.join("copies");
    backup::copy_with_policy(
        &mut work,
        &copies,
        &client.root,
        |f| {
            tauri::async_runtime::block_on(source.download(
                &client.runtime,
                &f.name[..36],
                &f.name[37..],
                f.size as i64,
                &f.sha256,
            ))
        },
        |f| backup::can_omit(&c, f),
    )
    .unwrap();
    backup::record_missing(&mut c, &before, &work.plan).unwrap();
    assert!(backup::asset_files(&work.plan)
        .iter()
        .any(|f| f.missing && f.name.starts_with(MISSING)));
    backup::rehearse(&copies, &work.plan).unwrap();
    backup::finish(&mut c, &work.plan, crate::db::now_millis()).unwrap();
    let local = backup::latest(&c).unwrap().unwrap();
    let binding = s3_sync::migration_source_binding(&c).unwrap();
    let mut snapshot = cloud::prepare(
        &mut c,
        &local,
        &binding,
        MAIN,
        &remote,
        crate::db::now_millis(),
    )
    .unwrap();
    let before = snapshot.clone();
    cloud::copy_with_policy(
        &copies,
        &local,
        &mut snapshot,
        &remote,
        |f| {
            tauri::async_runtime::block_on(source.download(
                &client.runtime,
                &f.name[..36],
                &f.name[37..],
                f.size as i64,
                &f.sha256,
            ))
        },
        true,
    )
    .unwrap();
    cloud::record_missing(&mut c, &local, &before, &snapshot, &remote).unwrap();
    cloud::finish(&mut c, &local, &snapshot, crate::db::now_millis()).unwrap();
    let snapshot = cloud::latest(&c).unwrap().unwrap();
    let plan = publication::prepare(&mut c, &local, &snapshot).unwrap();
    let token = publication::plan_digest(&plan).unwrap();
    publication::record(&mut c, &local, &snapshot, &plan, &token, None, false).unwrap();
    let plan = publication::latest(&c).unwrap().unwrap();
    let c = RefCell::new(c);
    publication::publish(
        &plan,
        &mut s3_sync::MigrationStagingTarget::new(&source, &plan.operation).unwrap(),
        |f| cloud::read_file(&copies, &local, &snapshot, f),
        || publication::require_current(&c.borrow(), &local, &snapshot, &plan),
        || {
            cloud::require_remote(
                &snapshot,
                &tauri::async_runtime::block_on(source.metadata())?,
            )
        },
        |done, published| {
            publication::record(
                &mut c.borrow_mut(),
                &local,
                &snapshot,
                &plan,
                &token,
                done,
                published,
            )
        },
    )
    .unwrap();
    let claim = space::Claim::from_plan(&plan).unwrap();
    let mut target = s3_sync::MigrationSpaceTarget::new(&source, Some(claim.clone())).unwrap();
    let ledger = space::initialize(&mut target, &claim, || {
        backup::require_current(&c.borrow(), &local)
    })
    .unwrap();
    let pending = space::Pending {
        claim: claim.clone(),
        local,
        mode: "create".into(),
    };
    let token = pending.token().unwrap();
    assert_eq!(
        pending
            .require_confirmation(Some(&token), false)
            .unwrap_err(),
        "MIGRATION_MISSING_CONFIRMATION"
    );
    pending.require_confirmation(Some(&token), true).unwrap();
    assert_eq!(
        pending
            .require_confirmation(Some("stale"), true)
            .unwrap_err(),
        "MIGRATION_SPACE_CONFIRMATION"
    );
    space::save_pending(&c.borrow(), &pending).unwrap();
    space::activate(&mut c.borrow_mut(), &pending, &ledger).unwrap();
    drop(c);
    sync(&client);
    assert!(space::is_active(&client.db.connection.lock().unwrap()).unwrap());
    let after = tauri::async_runtime::block_on(source.metadata()).unwrap();
    cloud::require_remote(&snapshot, &after).unwrap();
    println!("ACTIVATION_DESKTOP_PREPARE_OK: real native backup, publication, initialization, atomic activation, admitted sync; old space unchanged");
}
#[test]
#[ignore = "Requires Harmony formal join and purge"]
fn verify() {
    let client = Client::new(TODO, NOTE);
    configure(&client.db.connection.lock().unwrap());
    let source = s3_sync::MigrationAssetSource::from_test_bucket(bucket(SECRET), MAIN);
    let claim = space::association(
        &mut s3_sync::MigrationSpaceTarget::new(&source, None).unwrap(),
        MAIN,
    )
    .unwrap()
    .unwrap();
    let mut target = s3_sync::MigrationSpaceTarget::new(&source, Some(claim.clone())).unwrap();
    let mut c = client.db.connection.lock().unwrap();
    let local = local_backup(&mut c, &client.root);
    let ledger =
        space::initialize(&mut target, &claim, || backup::require_current(&c, &local)).unwrap();
    assert!(ledger
        .terminals
        .iter()
        .any(|t| t.kind == "note" && t.uuid == NOTE));
    space::activate(
        &mut c,
        &space::Pending {
            claim,
            local,
            mode: "join".into(),
        },
        &ledger,
    )
    .unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM notes WHERE uuid=?1", [NOTE], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(c);
    sync(&client);
    assert_eq!(
        client
            .db
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM notes WHERE uuid=?1", [NOTE], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    println!("ACTIVATION_DESKTOP_VERIFY_OK: stale peer joins normally, applies terminal before exposing old body, no resurrection after normal admitted sync");
}

#[test]
#[ignore = "Requires Harmony formal creator in run-sync-core-s3.ps1"]
fn reverse() {
    let client = Client::new(TODO, NOTE);
    let mut c = client.db.connection.lock().unwrap();
    c.execute(
        "UPDATE todos SET deleted_at=200,updated_at=200 WHERE uuid=?1",
        [TODO],
    )
    .unwrap();
    let local_purge = crate::purge::prepare(&mut c, None, 201).unwrap();
    crate::purge::execute_batch(&mut c, &local_purge.operation_uuid, 202).unwrap();
    configure(&c);
    assert!(
        s3_sync::prepare_with_fixture_credentials(&c, bucket(SECRET)).is_ok(),
        "original spaces now have a terminal-aware sidecar"
    );
    let source = s3_sync::MigrationAssetSource::from_test_bucket(bucket(SECRET), MAIN);
    let claim = space::association(
        &mut s3_sync::MigrationSpaceTarget::new(&source, None).unwrap(),
        MAIN,
    )
    .unwrap()
    .unwrap();
    let mut target = s3_sync::MigrationSpaceTarget::new(&source, Some(claim.clone())).unwrap();
    let local = local_backup(&mut c, &client.root);
    let ledger =
        space::initialize(&mut target, &claim, || backup::require_current(&c, &local)).unwrap();
    space::activate(
        &mut c,
        &space::Pending {
            claim,
            local,
            mode: "join".into(),
        },
        &ledger,
    )
    .unwrap();
    drop(c);
    sync(&client);
    let mut c = client.db.connection.lock().unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM todos WHERE uuid=?1", [TODO], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let now = crate::db::now_millis();
    c.execute(
        "UPDATE notes SET deleted_at=?1,updated_at=?1 WHERE uuid=?2",
        params![now, NOTE],
    )
    .unwrap();
    let plan = crate::purge::prepare(&mut c, None, now + 1).unwrap();
    crate::purge::execute_batch(&mut c, &plan.operation_uuid, now + 2).unwrap();
    crate::purge::cleanup(&c, &client.assets, &plan.operation_uuid).unwrap();
    let pending = crate::purge::status(&c, &plan.operation_uuid).unwrap();
    assert!(pending.sync_pending);
    assert_eq!(pending.remote_pending, 2);
    drop(c);
    sync(&client);
    let c = client.db.connection.lock().unwrap();
    let done = crate::purge::status(&c, &plan.operation_uuid).unwrap();
    assert!(!done.sync_pending);
    assert_eq!(done.remote_pending, 0);
    assert!(crate::purge::unfinished(&c).unwrap().is_none());
    println!("ACTIVATION_DESKTOP_REVERSE_OK: local-only terminal joins without legacy upload, new-space user purge and honest complete status");
}
