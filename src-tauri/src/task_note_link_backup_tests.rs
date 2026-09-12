use super::*;
use crate::{task_note_link_protocol as protocol, task_note_link_store as links};

fn fixture() -> TodoExport {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/task-note-link-backup-v3.json"
    ))
    .unwrap()
}
fn empty() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn dump(db: &Connection) -> Vec<String> {
    [
        "groups",
        "todos",
        "notes",
        "note_attachments",
        "recurrence_rules",
        "recurrence_sync_state",
        "task_note_links",
        "task_note_link_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .map(|table| {
        let mut statement = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                Ok((0..columns)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        format!("{table}:{rows:?}")
    })
    .collect()
}

#[test]
fn link_backup_roundtrip_preview_legacy_and_tombstone_protection() {
    let mut db = empty();
    let doc = fixture();
    let preview = build_preview(&db, Path::new("fixture.json"), &doc).unwrap();
    assert_eq!(
        (
            preview.link_total,
            preview.link_deleted,
            preview.link_metadata_included
        ),
        (1, 0, true)
    );
    merge_import(&mut db, doc).unwrap();
    assert_eq!(
        links::snapshot(&db).unwrap().document,
        fixture().task_note_links.unwrap()
    );
    let mut removed = fixture().task_note_links.unwrap();
    removed.links[0].updated_at = 200;
    removed.links[0].deleted_at = Some(200);
    links::merge(&mut db, &removed).unwrap();
    let mut newer_active = fixture();
    newer_active.task_note_links.as_mut().unwrap().links[0].updated_at = 500;
    merge_import(&mut db, newer_active).unwrap();
    assert_eq!(links::snapshot(&db).unwrap().document, removed);
    for version in [1, 2] {
        let before = links::snapshot(&db).unwrap();
        let mut old = fixture();
        old.format_version = version;
        old.task_note_links = None;
        if version == 1 {
            old.recurrence = None;
        }
        assert!(
            !build_preview(&db, Path::new("old.json"), &old)
                .unwrap()
                .link_metadata_included
        );
        merge_import(&mut db, old).unwrap();
        let after = links::snapshot(&db).unwrap();
        assert_eq!(
            (
                after.document,
                after.revision,
                after.synced_revision,
                after.etag
            ),
            (
                before.document,
                before.revision,
                before.synced_revision,
                before.etag
            )
        );
    }
    let snapshot = links::snapshot(&db).unwrap();
    assert!(snapshot.revision > snapshot.synced_revision);
    assert_eq!(snapshot.etag, None);
}

#[test]
fn link_backup_preserves_missing_and_reconciles_legacy_entity_deletions() {
    for version in [1, 2, 3] {
        let mut db = empty();
        merge_import(&mut db, fixture()).unwrap();
        let mut deleted = fixture();
        deleted.format_version = version;
        if version < 3 {
            deleted.task_note_links = None;
        }
        if version == 1 {
            deleted.recurrence = None;
        }
        deleted.todos[0].deleted_at = Some(300);
        deleted.todos[0].updated_at = 300;
        merge_import(&mut db, deleted).unwrap();
        let removed = links::snapshot(&db).unwrap().document;
        assert!(removed.links[0].deleted_at.is_some());
        let mut restored = fixture();
        restored.todos[0].updated_at = 500;
        merge_import(&mut db, restored).unwrap();
        assert_eq!(links::snapshot(&db).unwrap().document, removed);
    }
    let mut db = empty();
    let mut missing = fixture();
    missing.todos.clear();
    missing.notes.clear();
    merge_import(&mut db, missing).unwrap();
    assert_eq!(
        links::snapshot(&db).unwrap().document,
        fixture().task_note_links.unwrap()
    );
}

#[test]
fn link_backup_invalid_inputs_and_late_failures_roll_back_every_domain() {
    let raw = serde_json::to_value(fixture()).unwrap();
    for kind in 0..9 {
        let mut value = raw.clone();
        match kind {
            0 => {
                value.as_object_mut().unwrap().remove("task_note_links");
            }
            1 => value["task_note_links"] = serde_json::Value::Null,
            2 => value["format_version"] = 4.into(),
            3 => value["format_version"] = 2.into(),
            4 => value["task_note_links"]["format_version"] = 2.into(),
            5 => value["task_note_links"]["links"][0]["uuid"] = "invalid".into(),
            6 => value["task_note_links"]["links"][0]["extra"] = true.into(),
            7 => {
                value["task_note_links"]["links"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("deleted_at");
            }
            _ => value["task_note_links"]["links"][0]["updated_at"] = serde_json::json!(1.5),
        }
        let mut db = empty();
        let before = dump(&db);
        let result = serde_json::from_value(value)
            .map_err(|e| e.to_string())
            .and_then(|doc| merge_import(&mut db, doc));
        assert!(result.is_err(), "kind {kind}");
        assert_eq!(dump(&db), before);
    }
    for trigger in [
        "CREATE TRIGGER fail_links BEFORE INSERT ON task_note_links BEGIN SELECT RAISE(ABORT,'link failure'); END",
        "CREATE TRIGGER fail_links BEFORE UPDATE OF synced_revision ON task_note_link_sync_state BEGIN SELECT RAISE(ABORT,'ack failure'); END",
    ] {
        let mut db=empty(); db.execute_batch(trigger).unwrap(); let before=dump(&db);
        assert!(merge_import(&mut db,fixture()).is_err()); assert_eq!(dump(&db),before);
        db.execute_batch("DROP TRIGGER fail_links").unwrap(); merge_import(&mut db,fixture()).unwrap();
    }
    let mut db = empty();
    db.execute_batch(&format!(
        "UPDATE task_note_link_sync_state SET revision={}",
        protocol::MAX_CLOCK
    ))
    .unwrap();
    let before = dump(&db);
    assert!(merge_import(&mut db, fixture()).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn link_backup_zip_and_json_share_validated_data_and_reject_tampering() {
    let mut doc = fixture();
    doc.attachment_files_included = true;
    let bytes = serde_json::to_vec(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("eggdone-link-backup-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("test.eggdone-backup");
    let manifest = BackupManifest {
        format_version: 1,
        created_at: 1000,
        data: backup_manifest_entry("data.json".into(), &bytes),
        assets: vec![],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    write_full_backup_archive(&file, &manifest_bytes, &bytes, &[]).unwrap();
    let validated = read_full_backup_archive(&file, None).unwrap();
    assert_eq!(validated.import.task_note_links, doc.task_note_links);
    let mut db = empty();
    merge_import(&mut db, validated.import).unwrap();
    assert_eq!(
        links::snapshot(&db).unwrap().document,
        fixture().task_note_links.unwrap()
    );
    doc.task_note_links.as_mut().unwrap().links.clear();
    write_full_backup_archive(
        &file,
        &manifest_bytes,
        &serde_json::to_vec(&doc).unwrap(),
        &[],
    )
    .unwrap();
    assert!(read_full_backup_archive(&file, None).is_err());
    fs::remove_dir_all(dir).unwrap();
}
