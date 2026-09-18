use super::*;

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
