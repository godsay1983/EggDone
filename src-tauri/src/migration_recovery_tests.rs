use super::*;

#[test]
#[ignore = "Read-only snapshot diagnosis; requires EGGDONE_RECOVERY_SNAPSHOT directory"]
fn diagnose_existing_snapshot_read_only() {
    let root = PathBuf::from(
        std::env::var("EGGDONE_RECOVERY_SNAPSHOT").expect("explicit snapshot directory"),
    );
    let plan: BackupPlan =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let data = fs::read(root.join("data.json")).unwrap();
    let snapshot: Snapshot = serde_json::from_slice(&data).unwrap();
    let mut fresh = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut fresh).unwrap();
    for table in &snapshot.tables {
        assert!(TABLES.contains(&table.name.as_str()));
        let stmt = fresh
            .prepare(&format!("SELECT * FROM {} LIMIT 0", table.name))
            .unwrap();
        if table.columns != stmt.column_names() {
            println!("RECOVERY_SCHEMA_ORDER_MISMATCH: {}", table.name);
        }
    }
    let mut target = Connection::open_in_memory().unwrap();
    let result = restore(&mut target, &data, &plan);
    println!("RECOVERY_READ_ONLY_RESULT: {:?}", result);
    assert!(
        result.is_ok(),
        "restore failed; only safe diagnostic codes printed"
    );
    validate(&plan).unwrap();
    let scratch = std::env::temp_dir().join(format!(
        "eggdone-recovery-diagnostic-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&scratch).unwrap();
    let drill = (|| {
        let backup = scratch.join(&plan.operation);
        fs::create_dir(&backup).map_err(io)?;
        fs::copy(root.join("manifest.json"), backup.join("manifest.json")).map_err(io)?;
        for file in plan.files.iter().filter(|f| !f.missing) {
            fs::copy(root.join(&file.name), backup.join(&file.name)).map_err(io)?;
        }
        rehearse(&scratch, &plan)
    })();
    let owned = fs::canonicalize(&scratch).unwrap();
    assert_eq!(
        owned.parent(),
        Some(fs::canonicalize(std::env::temp_dir()).unwrap().as_path())
    );
    assert!(owned
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("eggdone-recovery-diagnostic-"));
    fs::remove_dir_all(owned).unwrap();
    assert_eq!(drill, Ok(()));
    assert_eq!(fs::read(root.join("data.json")).unwrap(), data);
    println!("RECOVERY_ISOLATED_COPY_OK: reopen, available assets and cleanup; source snapshot read-only");
}

fn fixture() -> (PathBuf, Connection, Work) {
    let root = std::env::temp_dir().join(format!("eggdone-recovery-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute_batch("INSERT INTO todos(uuid,title,sort_order,completed,archived_at,created_at,updated_at,updated_by) VALUES('t','archived',0,1,2,1,2,'d');
        INSERT INTO notes(uuid,title,content,created_at,updated_at,updated_by) VALUES('n','note','old',1,1,'d');
        UPDATE notes SET content='new',deleted_at=2 WHERE uuid='n';
        INSERT INTO app_metadata(key,value) VALUES('draft','unsaved text');
        INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo','gone','operation',3);
        INSERT INTO purge_cleanup(attachment_uuid,note_uuid,original_path) VALUES('removed-file','gone-note','not-copied');
        INSERT INTO purge_plans(operation_uuid,target_epoch,created_at,total,attachments,bytes,state) VALUES('p','epoch',1,1,0,0,'running');
        INSERT INTO purge_targets(operation_uuid,kind,uuid,fingerprint) VALUES('p','todo','t','fingerprint');").unwrap();
    let uuid = "00000000-0000-4000-8000-000000000002";
    c.execute("INSERT INTO note_attachments(uuid,note_uuid,kind,display_name,mime_type,byte_size,sha256,preview_mime_type,preview_byte_size,preview_sha256,width,height,created_at,updated_at,updated_by,deleted_at)
        VALUES(?1,'n','image','picture','image/jpeg',4,?2,'image/jpeg',3,?3,1,1,1,1,'d',2)",rusqlite::params![uuid,digest(b"data"),digest(b"jpg")]).unwrap();
    let assets = root.join("note-assets");
    fs::create_dir(&assets).unwrap();
    let parent = assets.join(uuid);
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("original"), b"data").unwrap();
    fs::write(parent.join("preview.jpg"), b"jpg").unwrap();
    let work = prepare(&mut c, 10).unwrap();
    copy(&work, &root.join("backups"), &assets).unwrap();
    (root, c, work)
}

fn changed(work: &Work, change: impl FnOnce(&mut serde_json::Value)) -> (Vec<u8>, BackupPlan) {
    let mut data: serde_json::Value = serde_json::from_slice(&work.data).unwrap();
    change(&mut data);
    let bytes = serde_json::to_vec(&data).unwrap();
    let mut plan = work.plan.clone();
    plan.data_hash = digest(&bytes);
    plan.files[0].sha256 = plan.data_hash.clone();
    plan.files[0].size = bytes.len() as u64;
    (bytes, plan)
}

#[test]
fn legacy_v22_without_retry_counter_recovers_then_upgrades_without_data_loss() {
    let (root, mut c, _) = fixture();
    remove_planning_schema(&c);
    c.execute_batch(
        "ALTER TABLE purge_cleanup DROP COLUMN local_attempts;
        DELETE FROM schema_migrations WHERE version=23;",
    )
    .unwrap();
    let legacy = prepare(&mut c, 30).unwrap();
    copy(&legacy, &root.join("backups"), &root.join("note-assets")).unwrap();
    rehearse(&root.join("backups"), &legacy.plan).unwrap();
    let mut target = Connection::open_in_memory().unwrap();
    restore(&mut target, &legacy.data, &legacy.plan).unwrap();
    assert_eq!(
        target
            .query_row("SELECT local_attempts FROM purge_cleanup", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    verify_restored(&target, &legacy.data).unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(
        c.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        26
    );
    assert_eq!(
        c.query_row("SELECT local_attempts FROM purge_cleanup", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    verify_restored(&c, &legacy.data).unwrap();
    // ALTER appends the column; a fresh v22 definition puts it before remote_done.
    let upgraded = prepare(&mut c, 40).unwrap();
    copy(&upgraded, &root.join("backups"), &root.join("note-assets")).unwrap();
    rehearse(&root.join("backups"), &upgraded.plan).unwrap();
    drop(c);
    drop(target);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn v22_with_existing_retry_counter_keeps_values_and_upgrade_is_idempotent() {
    let (root, mut c, _) = fixture();
    remove_planning_schema(&c);
    c.execute_batch("UPDATE purge_cleanup SET local_attempts=7; DELETE FROM schema_migrations WHERE version=23;").unwrap();
    let before = capture(&c).unwrap();
    crate::db::migrate(&mut c).unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(
        c.query_row("SELECT local_attempts FROM purge_cleanup", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        7
    );
    verify_restored(&c, &before).unwrap();
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

fn remove_planning_schema(c: &Connection) {
    crate::db::remove_daily_plan_schema_for_test(c);
}

#[test]
fn planning_recovery_preserves_evidence_membership_and_receipts() {
    let (root, mut c, _) = fixture();
    c.execute_batch("INSERT INTO daily_plan_events VALUES('11111111-1111-4111-8111-111111111111','1:61:0:-:-');
        INSERT INTO daily_plans VALUES('11111111-1111-4111-8111-111111111111','2026-09-19',1,0,1,'a','1:61:0:-:-');
        INSERT INTO daily_plan_operations VALUES('operation','payload');").unwrap();
    let work = prepare(&mut c, 50).unwrap();
    let mut target = Connection::open_in_memory().unwrap();
    restore(&mut target, &work.data, &work.plan).unwrap();
    verify_restored(&target, &work.data).unwrap();
    assert_eq!(
        target
            .query_row("SELECT COUNT(*) FROM daily_plans", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(c);
    drop(target);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workflow_recovery_preserves_states_digests_and_independent_ack() {
    let (root, mut c, _) = fixture();
    c.execute_batch("INSERT INTO task_workflow_states VALUES('11111111-1111-4111-8111-111111111111','waiting','private',NULL,1,'a','');
        INSERT INTO task_workflow_operations VALUES('operation','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
        UPDATE task_workflow_sync_state SET synced_revision=revision,etag='\"workflow\"',generation=2;").unwrap();
    let work = prepare(&mut c, 50).unwrap();
    let mut target = Connection::open_in_memory().unwrap();
    restore(&mut target, &work.data, &work.plan).unwrap();
    verify_restored(&target, &work.data).unwrap();
    assert_eq!(
        target
            .query_row("SELECT reason FROM task_workflow_states", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "private"
    );
    assert_eq!(
        target
            .query_row("SELECT generation FROM task_workflow_sync_state", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(c);
    drop(target);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_does_not_accept_arbitrary_missing_or_duplicate_columns() {
    let (root, c, work) = fixture();
    for duplicate in [false, true] {
        let (bytes, plan) = changed(&work, |v| {
            let table = v["tables"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|t| t["name"] == "purge_cleanup")
                .unwrap();
            if duplicate {
                table["columns"][1] = table["columns"][0].clone();
            } else {
                table["columns"].as_array_mut().unwrap().pop();
                for row in table["rows"].as_array_mut().unwrap() {
                    row.as_array_mut().unwrap().pop();
                }
            }
        });
        let mut target = Connection::open_in_memory().unwrap();
        assert_eq!(
            restore(&mut target, &bytes, &plan).unwrap_err(),
            "MIGRATION_RECOVERY_COLUMNS"
        );
    }
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn isolated_recovery_reopens_all_rows_and_assets_without_touching_source() {
    let (root, c, work) = fixture();
    let before = capture(&c).unwrap();
    rehearse(&root.join("backups"), &work.plan).unwrap();
    assert_eq!(capture(&c).unwrap(), before);
    assert!(
        fs::read_dir(root.join("backups").join(&work.plan.operation))
            .unwrap()
            .all(|p| !p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("recovery-"))
    );
    let mut restored = Connection::open_in_memory().unwrap();
    restore(&mut restored, &work.data, &work.plan).unwrap();
    assert_eq!(capture(&restored).unwrap(), work.data);
    assert!(restored.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES('gone','resurrect',0,1,1,'d')",[]).unwrap_err().to_string().contains("PURGE_TERMINAL"));
    assert!(restored
        .execute("DELETE FROM lifecycle_terminals", [])
        .is_err());
    restored
        .execute("UPDATE notes SET title='changed' WHERE uuid='n'", [])
        .unwrap();
    let count: i64 = restored
        .query_row("SELECT COUNT(*) FROM note_history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    drop(c);
    drop(restored);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_refuses_existing_target_foreign_schema_and_bad_shape() {
    let (root, mut c, work) = fixture();
    let before = capture(&c).unwrap();
    assert_eq!(
        restore(&mut c, &work.data, &work.plan).unwrap_err(),
        "MIGRATION_RECOVERY_TARGET"
    );
    assert_eq!(before, capture(&c).unwrap());
    let changes: Vec<Box<dyn FnOnce(&mut serde_json::Value)>> = vec![
        Box::new(|v| v["schema"] = 99.into()),
        Box::new(|v| v["client"] = "harmony".into()),
        Box::new(|v| v["tables"].as_array_mut().unwrap().swap(0, 1)),
        Box::new(|v| v["tables"][0]["name"] = "todos; DROP TABLE notes".into()),
        Box::new(|v| v["tables"][0]["columns"][0] = "unknown".into()),
        Box::new(|v| v["tables"][0]["rows"][0][0] = true.into()),
        Box::new(|v| {
            v["tables"][0]["rows"][0].as_array_mut().unwrap().pop();
        }),
    ];
    for change in changes {
        let (bytes, plan) = changed(&work, change);
        let mut target = Connection::open_in_memory().unwrap();
        assert!(restore(&mut target, &bytes, &plan).is_err());
    }
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn conflicting_terminal_and_constraint_failure_roll_back_restored_rows() {
    let (root, c, work) = fixture();
    for column in ["uuid", "title"] {
        let (bytes, plan) = changed(&work, |v| {
            let table = &mut v["tables"][0];
            let index = table["columns"]
                .as_array()
                .unwrap()
                .iter()
                .position(|c| c == column)
                .unwrap();
            table["rows"][0][index] = if column == "uuid" {
                "gone".into()
            } else {
                "".into()
            };
        });
        let mut target = Connection::open_in_memory().unwrap();
        assert!(restore(&mut target, &bytes, &plan).is_err());
        assert_eq!(
            target
                .query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            target
                .query_row("SELECT COUNT(*) FROM lifecycle_terminals", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(target.is_autocommit());
    }
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupted_snapshot_or_attachment_cannot_pass_recovery() {
    let (root, c, work) = fixture();
    let backup = root.join("backups").join(&work.plan.operation);
    fs::write(backup.join(&work.plan.files[1].name), b"xxxx").unwrap();
    assert!(rehearse(&root.join("backups"), &work.plan).is_err());
    copy(&work, &root.join("backups"), &root.join("note-assets")).unwrap();
    fs::write(backup.join("data.json"), b"{}").unwrap();
    assert!(rehearse(&root.join("backups"), &work.plan).is_err());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
