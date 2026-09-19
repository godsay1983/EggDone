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
fn deleted_missing_assets_are_explicit_and_rechecked() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let assets = source(&c, &root);
    let original = assets.join("00000000-0000-4000-8000-000000000002/original");
    fs::remove_file(&original).unwrap();
    let backup = root.join("migration-backups");
    let mut work = prepare(&mut c, 10).unwrap();
    let before = work.plan.clone();
    copy_with_policy(
        &mut work,
        &backup,
        &assets,
        |_| Err("MIGRATION_BACKUP_ASSET_NOT_FOUND".into()),
        |f| can_omit(&c, f),
    )
    .unwrap();
    assert!(work.plan.files[1].missing);
    assert!(!backup
        .join(&work.plan.operation)
        .join(&work.plan.files[1].name)
        .exists());
    record_missing(&mut c, &before, &work.plan).unwrap();
    rehearse(&backup, &work.plan).unwrap();
    finish(&mut c, &work.plan, 20).unwrap();
    assert!(latest(&c).unwrap().unwrap().files[1].missing);
    let mut retry = prepare(&mut c, 30).unwrap();
    assert!(!retry.plan.files[1].missing);
    let before = retry.plan.clone();
    fs::write(&original, b"data").unwrap();
    copy_with_policy(
        &mut retry,
        &backup,
        &assets,
        |_| panic!("valid local copy"),
        |f| can_omit(&c, f),
    )
    .unwrap();
    record_missing(&mut c, &before, &retry.plan).unwrap();
    assert!(!retry.plan.files[1].missing);
    rehearse(&backup, &retry.plan).unwrap();
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_policy_rejects_active_corrupt_denied_and_network_errors() {
    for case in ["active", "corrupt", "denied", "network", "invalid"] {
        let root = directory_fixture();
        let mut c = database(&root.join("db.sqlite"));
        let assets = source(&c, &root);
        let original = assets.join("00000000-0000-4000-8000-000000000002/original");
        if case == "corrupt" {
            fs::write(&original, b"xxxx").unwrap();
        } else {
            fs::remove_file(&original).unwrap();
        }
        if case == "active" {
            c.execute("UPDATE note_attachments SET deleted_at=NULL", [])
                .unwrap();
        }
        let mut work = prepare(&mut c, 10).unwrap();
        let error = match case {
            "denied" => "MIGRATION_BACKUP_ASSET_DENIED",
            "network" => "MIGRATION_BACKUP_ASSET_DOWNLOAD",
            "invalid" => "MIGRATION_BACKUP_ASSET_INVALID",
            _ => "MIGRATION_BACKUP_ASSET_NOT_FOUND",
        };
        assert!(
            copy_with_policy(
                &mut work,
                &root.join("migration-backups"),
                &assets,
                |_| Err(error.into()),
                |f| can_omit(&c, f)
            )
            .is_err(),
            "{case}"
        );
        assert!(!work.plan.files[1].missing);
        assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
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
        missing: false,
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

#[test]
fn missing_deleted_asset_download_is_verified_and_never_replaces_live_cache() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let assets = source(&c, &root);
    let uuid = "00000000-0000-4000-8000-000000000002";
    let original = assets.join(uuid).join("original");
    fs::write(&original, b"corrupt").unwrap();
    let work = prepare(&mut c, 10).unwrap();
    let backup = root.join("migration-backups");
    let mut requests = 0;
    copy_with_missing(&work, &backup, &assets, |entry| {
        assert_eq!(entry.name, format!("{uuid}-original"));
        require_current(&c, &work.plan)?;
        requests += 1;
        Ok(b"data".to_vec())
    })
    .unwrap();
    assert_eq!(requests, 1);
    assert_eq!(fs::read(&original).unwrap(), b"corrupt");
    rehearse(&backup, &work.plan).unwrap();
    finish(&mut c, &work.plan, 20).unwrap();
    drop(c);
    let mut c = database(&root.join("db.sqlite"));
    let retry = prepare(&mut c, 30).unwrap();
    assert_eq!(retry.plan.operation, work.plan.operation);
    copy_with_missing(&retry, &backup, &assets, |_| {
        panic!("valid copy must not redownload")
    })
    .unwrap();
    finish(&mut c, &retry.plan, 40).unwrap();
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn download_failure_corruption_and_target_change_cannot_certify_snapshot() {
    let root = directory_fixture();
    let mut c = database(&root.join("db.sqlite"));
    let assets = source(&c, &root);
    fs::remove_file(assets.join("00000000-0000-4000-8000-000000000002/original")).unwrap();
    let work = prepare(&mut c, 10).unwrap();
    let backup = root.join("migration-backups");
    assert_eq!(
        copy_with_missing(&work, &backup, &assets, |_| Err(
            "MIGRATION_BACKUP_ASSET_DOWNLOAD".into()
        ))
        .unwrap_err(),
        "MIGRATION_BACKUP_ASSET_DOWNLOAD"
    );
    for bytes in [b"bad".to_vec(), b"xxxx".to_vec()] {
        assert_eq!(
            copy_with_missing(&work, &backup, &assets, |_| Ok(bytes.clone())).unwrap_err(),
            "MIGRATION_BACKUP_ASSET_INVALID"
        );
        assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    }
    copy_with_missing(&work, &backup, &assets, |_| {
        c.execute(
            "UPDATE sync_settings SET endpoint='https://changed.example'",
            [],
        )
        .unwrap();
        Ok(b"data".to_vec())
    })
    .unwrap();
    assert_eq!(
        require_current(&c, &work.plan).unwrap_err(),
        "MIGRATION_BACKUP_CHANGED"
    );
    assert_eq!(
        finish(&mut c, &work.plan, 20).err().unwrap(),
        "MIGRATION_BACKUP_CHANGED"
    );
    assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
