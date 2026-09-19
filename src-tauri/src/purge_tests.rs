use crate::purge::{self, Target};
use rusqlite::Connection;

const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
fn seed() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/trash-restore-v1.json")).unwrap();
    for sql in fixture["seed"].as_array().unwrap() {
        db.execute_batch(sql.as_str().unwrap()).unwrap();
    }
    db
}
fn target(kind: &str, uuid: &str) -> Target {
    Target {
        kind: kind.into(),
        uuid: uuid.into(),
    }
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn purge_fixed_snapshot_and_terminal_guards() {
    let mut db = seed();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    assert_eq!(plan.total, 2);
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,deleted_at) VALUES(?1,'late',0,1,1,'test',10)",["123e4567-e89b-42d3-a456-426614174099"]).unwrap();
    let result = purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap();
    assert_eq!((result.purged, result.pending, result.skipped), (2, 0, 0));
    assert_eq!(count(&db, "todos"), 1);
    assert_eq!(count(&db, "notes"), 0);
    assert_eq!(count(&db, "note_attachments"), 0);
    assert_eq!(count(&db, "lifecycle_terminals"), 2);
    assert_eq!(
        purge::execute_batch(&mut db, &plan.operation_uuid, 201).unwrap(),
        result
    );
    assert!(db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'revive',0,1,9007199254740991,'old')",[TODO]).is_err());
    assert!(db.execute("DELETE FROM lifecycle_terminals", []).is_err());
}

#[test]
fn purge_skips_restored_and_changed_records() {
    let mut db = seed();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    db.execute("UPDATE todos SET deleted_at=NULL WHERE uuid=?1", [TODO])
        .unwrap();
    db.execute(
        "UPDATE notes SET content='concurrent' WHERE uuid=?1",
        [NOTE],
    )
    .unwrap();
    let result = purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap();
    assert_eq!((result.purged, result.skipped), (0, 2));
    assert_eq!(count(&db, "lifecycle_terminals"), 0);
}

#[test]
fn purge_is_atomic_and_retries_the_same_operation() {
    let mut db = seed();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    db.execute_batch("CREATE TRIGGER fail_purge BEFORE DELETE ON todos BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(purge::execute_batch(&mut db, &plan.operation_uuid, 200).is_err());
    assert_eq!(count(&db, "lifecycle_terminals"), 0);
    assert_eq!(count(&db, "notes"), 1);
    assert_eq!(count(&db, "purge_cleanup"), 0);
    assert_eq!(purge::status(&db, &plan.operation_uuid).unwrap().pending, 2);
    db.execute_batch("DROP TRIGGER fail_purge").unwrap();
    assert_eq!(
        purge::execute_batch(&mut db, &plan.operation_uuid, 200)
            .unwrap()
            .purged,
        2
    );
}

#[test]
fn purge_original_space_works_offline_without_migration() {
    let mut db = seed();
    db.execute(
        "UPDATE sync_settings SET enabled=0,endpoint='https://example.invalid'",
        [],
    )
    .unwrap();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    assert_eq!(
        purge::execute_batch(&mut db, &plan.operation_uuid, 200)
            .unwrap()
            .purged,
        2
    );
    assert_eq!(count(&db, "lifecycle_terminals"), 2);
}

#[test]
fn purge_does_not_touch_archives_or_linked_entities() {
    let mut db = seed();
    db.execute(
        "UPDATE todos SET deleted_at=NULL,archived_at=50 WHERE uuid=?1",
        [TODO],
    )
    .unwrap();
    let plan = purge::prepare(&mut db, Some(vec![target("note", NOTE)]), 100).unwrap();
    purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap();
    assert_eq!(count(&db, "todos"), 1);
    assert_eq!(count(&db, "lifecycle_terminals"), 1);
}

#[test]
fn purge_plan_never_stores_plaintext_and_rejects_duplicate_targets() {
    let mut db = seed();
    assert_eq!(
        purge::prepare(
            &mut db,
            Some(vec![target("todo", TODO), target("todo", TODO)]),
            100
        )
        .unwrap_err(),
        "PURGE_DUPLICATE_TARGET"
    );
    assert_eq!(count(&db, "purge_plans"), 0);
    let plan = purge::prepare(&mut db, Some(vec![target("todo", TODO)]), 100).unwrap();
    let digest: String = db
        .query_row(
            "SELECT fingerprint FROM purge_targets WHERE operation_uuid=?1",
            [&plan.operation_uuid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn purge_resumes_after_reopen_without_repeating_committed_batches() {
    let path = std::env::temp_dir().join(format!("eggdone-purge-{}.sqlite", uuid::Uuid::new_v4()));
    let operation = {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        for _ in 0..105 {
            db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,deleted_at) VALUES(?1,'isolated',0,1,1,'test',10)", [uuid::Uuid::new_v4().to_string()]).unwrap();
        }
        let plan = purge::prepare(&mut db, None, 100).unwrap();
        let first = purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap();
        assert_eq!((first.purged, first.pending), (50, 55));
        plan.operation_uuid
    };
    let mut db = Connection::open(&path).unwrap();
    crate::db::migrate(&mut db).unwrap();
    assert_eq!(
        purge::unfinished(&db).unwrap().unwrap().operation_uuid,
        operation
    );
    db.execute_batch("CREATE TRIGGER fail_purge BEFORE DELETE ON todos BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(purge::execute_batch(&mut db, &operation, 300).is_err());
    assert_eq!(count(&db, "lifecycle_terminals"), 50);
    db.execute_batch("DROP TRIGGER fail_purge").unwrap();
    assert_eq!(
        purge::execute_batch(&mut db, &operation, 400)
            .unwrap()
            .pending,
        5
    );
    assert_eq!(
        purge::execute_batch(&mut db, &operation, 500)
            .unwrap()
            .purged,
        105
    );
    assert!(purge::unfinished(&db).unwrap().is_none());
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn purge_blocks_pending_creation_and_scrubs_nested_receipts() {
    let mut db = seed();
    let payload =
        serde_json::json!({"items":[{"uuid":TODO,"title":"private receipt text"}]}).to_string();
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('task.batch.pending.v1',?1)",
        [&payload],
    )
    .unwrap();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    assert_eq!(
        purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap_err(),
        "PURGE_PENDING_CREATION"
    );
    assert_eq!(count(&db, "lifecycle_terminals"), 0);
    db.execute(
        "DELETE FROM app_metadata WHERE key='task.batch.pending.v1'",
        [],
    )
    .unwrap();
    let nested = serde_json::json!({"fingerprint":payload}).to_string();
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('task.batch.create.v1:test',?1)",
        [&nested],
    )
    .unwrap();
    purge::execute_batch(&mut db, &plan.operation_uuid, 300).unwrap();
    let receipt: String = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key='task.batch.create.v1:test'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(receipt, "{\"purged\":true}");
}

#[test]
fn purge_cleanup_retries_without_trusting_persisted_paths() {
    let root = std::env::temp_dir().join(format!("eggdone-purge-assets-{}", uuid::Uuid::new_v4()));
    let asset_root = root.join("note-assets");
    std::fs::create_dir_all(&asset_root).unwrap();
    let outside = root.join("keep.txt");
    std::fs::write(&outside, b"keep").unwrap();
    let invalid_asset = asset_root.join("123e4567-e89b-42d3-a456-426614174002");
    // A file where a UUID directory should be is not recursively deleted.
    std::fs::write(&invalid_asset, b"invalid").unwrap();
    let store = crate::note_asset_store::NoteAssetStore::for_root(root.clone());
    let mut db = seed();
    db.execute(
        "UPDATE note_attachments SET local_original_path=?1",
        [outside.to_str().unwrap()],
    )
    .unwrap();
    let plan = purge::prepare(&mut db, None, 100).unwrap();
    purge::execute_batch(&mut db, &plan.operation_uuid, 200).unwrap();
    purge::cleanup(&db, &store, &plan.operation_uuid).unwrap();
    assert_eq!(
        purge::status(&db, &plan.operation_uuid)
            .unwrap()
            .cleanup_pending,
        1
    );
    assert!(purge::unfinished(&db).unwrap().is_some());
    assert!(outside.exists());
    std::fs::remove_file(&invalid_asset).unwrap();
    std::fs::create_dir(&invalid_asset).unwrap();
    std::fs::write(invalid_asset.join("original"), b"private").unwrap();
    purge::cleanup(&db, &store, &plan.operation_uuid).unwrap();
    assert_eq!(
        purge::status(&db, &plan.operation_uuid)
            .unwrap()
            .cleanup_pending,
        0
    );
    assert!(!invalid_asset.exists());
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    std::fs::remove_file(outside).unwrap();
    std::fs::remove_dir(asset_root).unwrap();
    std::fs::remove_dir(root).unwrap();
}
