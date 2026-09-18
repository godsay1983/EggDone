use super::*;
use crate::db;

fn database(path: &Path) -> Connection {
    let mut c = Connection::open(path).unwrap();
    db::migrate(&mut c).unwrap();
    c
}
fn directory_fixture() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("eggdone-migration-backup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}
fn source(c: &Connection, root: &Path) -> PathBuf {
    let uuid = "00000000-0000-4000-8000-000000000002";
    c.execute_batch("INSERT INTO notes(uuid,title,content,created_at,updated_at,updated_by,deleted_at) VALUES('n','note','keep',1,1,'d',2)").unwrap();
    c.execute("INSERT INTO note_attachments(uuid,note_uuid,kind,display_name,mime_type,byte_size,sha256,created_at,updated_at,updated_by,deleted_at) VALUES(?1,'n','file','file','text/plain',4,?2,1,1,'d',2)",rusqlite::params![uuid,digest(b"data")]).unwrap();
    let assets = root.join("note-assets");
    fs::create_dir(&assets).unwrap();
    let parent = assets.join(uuid);
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("original"), b"data").unwrap();
    assets
}
#[test]
fn verified_snapshot_keeps_deleted_assets_and_local_domains_across_reopen() {
    let root = directory_fixture();
    let path = root.join("db.sqlite");
    let mut c = database(&path);
    let assets = source(&c, &root);
    c.execute(
        "INSERT INTO app_metadata(key,value) VALUES('task.batch.pending.v1','private draft')",
        [],
    )
    .unwrap();
    let work = prepare(&mut c, 10).unwrap();
    let snapshot: serde_json::Value = serde_json::from_slice(&work.data).unwrap();
    assert_eq!(snapshot["tables"].as_array().unwrap().len(), TABLES.len());
    assert!(String::from_utf8_lossy(&work.data).contains("private draft"));
    let backup = root.join("migration-backups");
    copy(&work, &backup, &assets).unwrap();
    assert_eq!(finish(&mut c, &work.plan, 20).unwrap().files, 1);
    let operation = work.plan.operation.clone();
    drop(c);
    let mut c = database(&path);
    let plan = latest(&c).unwrap().unwrap();
    assert_eq!(plan.operation, operation);
    verify_files(&backup, &plan).unwrap();
    assert!(report(&mut c, &plan).unwrap().current);
    assert_eq!(prepare(&mut c, 30).unwrap().plan.operation, operation);
    drop(c);
    fs::remove_dir_all(&root).unwrap();
}
#[test]
fn missing_and_corrupt_files_never_acknowledge_a_backup_and_resume_same_operation() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let assets = source(&c, &root);
    let work = prepare(&mut c, 10).unwrap();
    let backup = root.join("migration-backups");
    let original = assets.join("00000000-0000-4000-8000-000000000002/original");
    fs::remove_file(&original).unwrap();
    assert!(copy(&work, &backup, &assets).unwrap_err().contains("ASSET"));
    assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    fs::write(&original, b"data").unwrap();
    let retry = prepare(&mut c, 11).unwrap();
    assert_eq!(retry.plan.operation, work.plan.operation);
    copy(&retry, &backup, &assets).unwrap();
    finish(&mut c, &retry.plan, 20).unwrap();
    let copied = backup
        .join(&retry.plan.operation)
        .join(&retry.plan.files[1].name);
    fs::write(&copied, b"xxxx").unwrap();
    assert_eq!(
        verify_files(&backup, &retry.plan).unwrap_err(),
        "MIGRATION_BACKUP_FILE"
    );
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn source_edits_and_changed_targets_invalidate_confirmation_without_clearing_dirty() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let work = prepare(&mut c, 10).unwrap();
    copy(
        &work,
        &root.join("migration-backups"),
        &root.join("absent-assets"),
    )
    .unwrap();
    c.execute(
        "INSERT INTO notes(uuid,title,created_at,updated_at,updated_by) VALUES('n','new',1,1,'d')",
        [],
    )
    .unwrap();
    assert_eq!(
        finish(&mut c, &work.plan, 20).err().unwrap(),
        "MIGRATION_BACKUP_CHANGED"
    );
    let report = report(&mut c, &work.plan).unwrap();
    assert!(!report.current);
    assert!(report.blockers.contains(&"pending_notes".into()));
    let second = prepare(&mut c, 30).unwrap();
    assert_ne!(work.plan.operation, second.plan.operation);
    c.execute(
        "INSERT INTO app_metadata(key,value) VALUES('sync.target.epoch.v1','changed')",
        [],
    )
    .unwrap();
    assert_eq!(
        finish(&mut c, &second.plan, 40).err().unwrap(),
        "MIGRATION_BACKUP_CHANGED"
    );
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn incomplete_output_and_plan_corruption_are_not_accepted() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let work = prepare(&mut c, 10).unwrap();
    let target = root.join("migration-backups");
    fs::write(&target, b"not a directory").unwrap();
    assert!(copy(&work, &target, &root).is_err());
    assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    let mut corrupt = work.plan.clone();
    corrupt.files.push(FileEntry {
        name: "../outside".into(),
        size: 1,
        sha256: digest(b"a"),
    });
    assert!(validate(&corrupt).is_err());
    corrupt.files[1].name = "界".repeat(20);
    assert!(validate(&corrupt).is_err());
    c.execute("UPDATE app_metadata SET value='{}' WHERE key=?1", [KEY])
        .unwrap();
    assert!(latest(&c).is_err());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn source_ledger_history_and_receipts_are_bound_not_only_preflight_counts() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    c.execute_batch(
        "INSERT INTO app_metadata(key,value) VALUES('recurrence.instance.v1:sample','old')",
    )
    .unwrap();
    let first = prepare(&mut c, 10).unwrap();
    c.execute_batch(
        "UPDATE app_metadata SET value='new' WHERE key='recurrence.instance.v1:sample'",
    )
    .unwrap();
    assert!(!report(&mut c, &first.plan).unwrap().current);
    c.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,20)",rusqlite::params![uuid::Uuid::new_v4().to_string(),uuid::Uuid::new_v4().to_string()]).unwrap();
    let second = prepare(&mut c, 30).unwrap();
    copy(&second, &root.join("migration-backups"), &root).unwrap();
    assert!(finish(&mut c, &second.plan, 40)
        .unwrap()
        .blockers
        .contains(&"local_purge_terminals".into()));
    assert_eq!(crate::purge::require_safe(&c), Ok(()));
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
