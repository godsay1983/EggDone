use super::*;

fn absent() -> Vec<RemoteObject> {
    (0..8)
        .map(|_| RemoteObject {
            etag: None,
            bytes: None,
        })
        .collect()
}
fn setup() -> (PathBuf, Connection, BackupPlan, String) {
    let root = std::env::temp_dir().join(format!("eggdone-cloud-backup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let mut c = Connection::open(root.join("db.sqlite")).unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute("UPDATE sync_settings SET endpoint='https://example.com',region='us-east-1',bucket='fixture',object_key='account/todos.json'", []).unwrap();
    let source = crate::s3_sync::migration_source_binding(&c).unwrap();
    let work = super::super::prepare(&mut c, 1).unwrap();
    super::super::copy(&work, &root.join("backups"), &root.join("assets")).unwrap();
    super::super::finish(&mut c, &work.plan, 2).unwrap();
    let local = super::super::latest(&c).unwrap().unwrap();
    (root, c, local, source)
}
#[test]
fn cloud_snapshot_reopens_reuses_and_preserves_original_bytes() {
    let (root, mut c, local, source) = setup();
    let mut remote = absent();
    remote[3] = RemoteObject {
        etag: Some("\"one\"".into()),
        bytes: Some(b"{\n  \"format_version\":1, \"rules\":[]\n}".to_vec()),
    };
    let plan = prepare(&mut c, &local, &source, "account/todos.json", &remote, 3).unwrap();
    copy(&root.join("backups"), &local, &plan, &remote, |_| {
        panic!("no assets")
    })
    .unwrap();
    require_remote(&plan, &remote).unwrap();
    finish(&mut c, &local, &plan, 4).unwrap();
    assert_eq!(
        fs::read(
            cloud_folder(&root.join("backups"), &local, &plan, false)
                .unwrap()
                .join("meta-3.json")
        )
        .unwrap(),
        remote[3].bytes.clone().unwrap()
    );
    drop(c);
    let mut c = Connection::open(root.join("db.sqlite")).unwrap();
    let saved = latest(&c).unwrap().unwrap();
    assert_eq!(saved.verified_at, Some(4));
    assert!(report(&c, &local).unwrap().unwrap().current);
    let retry = prepare(&mut c, &local, &source, "account/todos.json", &remote, 5).unwrap();
    assert_eq!(retry.operation, plan.operation);
    assert!(retry.verified_at.is_none());
    remote[3].etag = Some("\"two\"".into());
    assert_eq!(
        require_remote(&retry, &remote).unwrap_err(),
        "MIGRATION_CLOUD_CHANGED"
    );
    let next = prepare(&mut c, &local, &source, "account/todos.json", &remote, 6).unwrap();
    assert_ne!(next.operation, plan.operation);
    c.execute("UPDATE sync_settings SET bucket='changed'", [])
        .unwrap();
    assert!(finish(&mut c, &local, &next, 7).is_err());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn cloud_snapshot_rejects_invalid_input_and_corrupt_private_files() {
    let (root, mut c, local, source) = setup();
    let mut remote = absent();
    remote[0].etag = Some("\"only-header\"".into());
    assert!(prepare(&mut c, &local, &source, "account/todos.json", &remote, 3).is_err());
    remote = absent();
    remote[3] = RemoteObject {
        etag: Some("W/\"weak\"".into()),
        bytes: Some(b"{\"format_version\":1,\"rules\":[]}".to_vec()),
    };
    assert!(prepare(&mut c, &local, &source, "account/todos.json", &remote, 3).is_err());
    remote[3].etag = Some("\"good\"".into());
    let plan = prepare(&mut c, &local, &source, "account/todos.json", &remote, 3).unwrap();
    copy(&root.join("backups"), &local, &plan, &remote, |_| panic!()).unwrap();
    let path = cloud_folder(&root.join("backups"), &local, &plan, false)
        .unwrap()
        .join("meta-3.json");
    fs::write(&path, b"broken").unwrap();
    assert!(verify_files(&root.join("backups"), &local, &plan).is_err());
    assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    copy(&root.join("backups"), &local, &plan, &remote, |_| panic!()).unwrap();
    verify_files(&root.join("backups"), &local, &plan).unwrap();
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cloud_only_deleted_assets_must_be_complete_and_absence_is_versioned() {
    let (root, mut c, local, source) = setup();
    let mut remote = absent();
    let initial = prepare(&mut c, &local, &source, "account/todos.json", &remote, 3).unwrap();
    let doc = serde_json::json!({"format_version":1,"device_id":"00000000-0000-4000-8000-000000000001","generated_at":2,
      "attachments":[{"uuid":"00000000-0000-4000-8000-000000000002","note_uuid":"00000000-0000-4000-8000-000000000003",
      "kind":"file","display_name":"gone.txt","mime_type":"text/plain","byte_size":4,"sha256":digest(b"data"),
      "sort_order":0,"created_at":1,"updated_at":2,"deleted_at":2,"updated_by":"00000000-0000-4000-8000-000000000001"}]});
    remote[2] = RemoteObject {
        etag: Some("\"asset\"".into()),
        bytes: Some(serde_json::to_vec(&doc).unwrap()),
    };
    assert!(require_remote(&initial, &remote).is_err());
    let plan = prepare(&mut c, &local, &source, "account/todos.json", &remote, 4).unwrap();
    assert_eq!(plan.assets.len(), 1);
    assert!(copy(&root.join("backups"), &local, &plan, &remote, |_| Err(
        "missing".into()
    ))
    .is_err());
    assert!(latest(&c).unwrap().unwrap().verified_at.is_none());
    assert_eq!(
        copy(&root.join("backups"), &local, &plan, &remote, |_| Ok(
            b"xxxx".to_vec()
        ))
        .unwrap_err(),
        "MIGRATION_BACKUP_ASSET_INVALID"
    );
    copy(&root.join("backups"), &local, &plan, &remote, |_| {
        Ok(b"data".to_vec())
    })
    .unwrap();
    copy(&root.join("backups"), &local, &plan, &remote, |_| {
        panic!("verified file should be reused")
    })
    .unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM note_attachments", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(require_remote(&plan, &absent()).is_err());
    let mut invalid = plan.clone();
    invalid.assets[0].name = "../escape".into();
    assert!(validate_cloud(&invalid).is_err());
    invalid = plan.clone();
    invalid.assets.push(invalid.assets[0].clone());
    assert!(validate_cloud(&invalid).is_err());
    invalid = plan.clone();
    invalid.assets[0].size = 21 * 1024 * 1024;
    assert!(validate_cloud(&invalid).is_err());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
