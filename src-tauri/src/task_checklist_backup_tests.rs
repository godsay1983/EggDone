use super::*;
use crate::{task_checklist_protocol as protocol, task_checklist_store as store};

fn fixture() -> TodoExport {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/task-checklist-backup-v4.json"
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
        "task_checklist_items",
        "task_checklist_definitions",
        "task_checklist_sync_state",
        "task_checklist_operations",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .map(|table| {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let n = q.column_count();
        format!(
            "{table}:{:?}",
            q.query_map([], |r| Ok((0..n)
                .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                .collect::<Vec<_>>()))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        )
    })
    .collect()
}

#[test]
fn checklist_immediate_check_restore_invalidates_old_write_receipt() {
    let by = "00000000-0000-4000-8000-00000000000a";
    let mut db = empty();
    let id = fixture().todos[0].uuid.clone();
    merge_import(&mut db, fixture()).unwrap();
    let panel = crate::task_checklist_views::read(&mut db, &id).unwrap();
    let mut request = store::ChecklistSave {
        operation_uuid: uuid::Uuid::new_v4().to_string(),
        todo_uuid: id.clone(),
        expected_updated_at: panel.updated_at,
        title: panel.title,
        note: panel.note,
        items: panel
            .items
            .items
            .iter()
            .filter(|i| i.deleted_at.is_none())
            .map(|i| store::ChecklistEdit {
                uuid: i.uuid.clone(),
                content: i.content.clone(),
                sort_order: i.sort_order,
                completed: true,
            })
            .collect(),
        expected_items: panel.items,
    };
    store::save(&mut db, &request, 3000, by).unwrap();
    let checked = store::snapshot(&mut db).unwrap();
    let exported = capture_export(&mut db, false, 4000).unwrap();
    merge_import(&mut db, exported).unwrap();
    assert_eq!(store::snapshot(&mut db).unwrap().items, checked.items);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM task_checklist_operations", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    assert!(store::save(&mut db, &request, 4000, by)
        .unwrap_err()
        .contains("STALE"));
    let restored = crate::task_checklist_views::read(&mut db, &id).unwrap();
    assert!(restored.items.items[0].completed);
    request.operation_uuid = uuid::Uuid::new_v4().to_string();
    request.expected_updated_at = restored.updated_at;
    request.expected_items = restored.items;
    request.items[0].completed = false;
    store::save(&mut db, &request, 5000, by).unwrap();
    assert!(
        !crate::task_checklist_views::read(&mut db, &id)
            .unwrap()
            .items
            .items[0]
            .completed
    );
    assert!(!db
        .query_row("SELECT completed FROM todos WHERE uuid=?1", [&id], |r| r
            .get::<_, bool>(
            0
        ))
        .unwrap());
}

#[test]
fn checklist_backup_roundtrip_preview_and_legacy_preservation() {
    let mut db = empty();
    let p = build_preview(&db, Path::new("v4.json"), &fixture()).unwrap();
    assert_eq!(
        (
            p.checklist_total,
            p.checklist_deleted,
            p.checklist_definition_total,
            p.checklist_missing_parent_total,
            p.checklist_metadata_included
        ),
        (2, 1, 2, 1, true)
    );
    merge_import(&mut db, fixture()).unwrap();
    let exported = capture_export(&mut db, false, 4000).unwrap();
    assert_eq!(exported.format_version, 5);
    assert_eq!(
        exported.task_checklist_items,
        fixture().task_checklist_items
    );
    assert_eq!(
        exported.task_checklist_definitions,
        fixture().task_checklist_definitions
    );
    let text = serde_json::to_string(&exported).unwrap();
    for forbidden in ["etag", "synced_revision", "generation", "operation_uuid"] {
        assert!(!text.contains(forbidden));
    }
    let mut other = empty();
    merge_import(&mut other, serde_json::from_str(&text).unwrap()).unwrap();
    let a = store::snapshot(&mut db).unwrap();
    let b = store::snapshot(&mut other).unwrap();
    assert_eq!((&a.items, &a.definitions), (&b.items, &b.definitions));
    assert!(b.item_state.revision > b.item_state.synced_revision);
    assert!(b.definition_state.revision > b.definition_state.synced_revision);
    for version in 1..=3 {
        let before = store::snapshot(&mut db).unwrap();
        let mut old = fixture();
        old.format_version = version;
        old.task_checklist_items = None;
        old.task_checklist_definitions = None;
        if version < 3 {
            old.task_note_links = None;
        }
        if version < 2 {
            old.recurrence = None;
        }
        assert!(
            !build_preview(&db, Path::new("old.json"), &old)
                .unwrap()
                .checklist_metadata_included
        );
        merge_import(&mut db, old).unwrap();
        assert_eq!(store::snapshot(&mut db).unwrap(), before);
    }
    merge_import(&mut db, fixture()).unwrap();
    let after = store::snapshot(&mut db).unwrap();
    assert_eq!(after.items, a.items);
    assert_eq!(after.definitions, a.definitions);
    assert!(after.item_state.generation > a.item_state.generation);
}

#[test]
fn checklist_backup_tombstones_empty_domains_and_missing_parent() {
    let mut db = empty();
    merge_import(&mut db, fixture()).unwrap();
    let mut doc = fixture();
    doc.task_checklist_items.as_mut().unwrap().items[1].deleted_at = None;
    doc.task_checklist_items.as_mut().unwrap().items[1].updated_at = 9000;
    doc.task_checklist_definitions.as_mut().unwrap().definitions[1].deleted_at = None;
    doc.task_checklist_definitions.as_mut().unwrap().definitions[1].updated_at = 9000;
    merge_import(&mut db, doc).unwrap();
    let s = store::snapshot(&mut db).unwrap();
    assert!(s.items.items[1].deleted_at.is_some());
    assert!(s.definitions.definitions[1].deleted_at.is_some());
    let mut deleted = fixture();
    deleted.todos[0].deleted_at = Some(10000);
    deleted.todos[0].updated_at = 10000;
    let parent = deleted.todos[0].uuid.clone();
    merge_import(&mut db, deleted).unwrap();
    assert!(store::visible_items(&mut db, &parent).unwrap().is_empty());
    let mut none = fixture();
    none.task_checklist_items = Some(Default::default());
    none.task_checklist_definitions = Some(Default::default());
    merge_import(&mut db, none).unwrap();
    assert_eq!(store::snapshot(&mut db).unwrap().items, s.items);
    let mut fresh = empty();
    let out = capture_export(&mut fresh, false, 1000).unwrap();
    assert!(out.task_checklist_items.unwrap().items.is_empty());
}

#[test]
fn checklist_backup_invalid_and_late_failures_are_atomic() {
    let raw = serde_json::to_value(fixture()).unwrap();
    for kind in 0..12 {
        let mut v = raw.clone();
        match kind {
            0 => {
                v.as_object_mut().unwrap().remove("task_checklist_items");
            }
            1 => v["task_checklist_items"] = serde_json::Value::Null,
            2 => {
                v.as_object_mut()
                    .unwrap()
                    .remove("task_checklist_definitions");
            }
            3 => v["task_checklist_definitions"] = serde_json::Value::Null,
            4 => v["task_checklist_items"]["format_version"] = 2.into(),
            5 => v["task_checklist_definitions"]["format_version"] = 2.into(),
            6 => v["extra"] = true.into(),
            7 => v["task_checklist_items"]["items"][0]["extra"] = true.into(),
            8 => v["task_checklist_items"]["items"][0]["uuid"] = "invalid".into(),
            9 => v["format_version"] = 1.into(),
            10 => v["format_version"] = 2.into(),
            _ => v["format_version"] = 3.into(),
        }
        let mut db = empty();
        let before = dump(&db);
        let r = serde_json::from_value(v)
            .map_err(|e| e.to_string())
            .and_then(|d| merge_import(&mut db, d));
        assert!(r.is_err(), "case {kind}");
        assert_eq!(dump(&db), before);
    }
    for target in [
        "BEFORE INSERT ON task_checklist_items",
        "BEFORE INSERT ON task_checklist_definitions",
        "BEFORE UPDATE OF synced_revision ON task_checklist_sync_state",
    ] {
        let mut db = empty();
        db.execute_batch(&format!(
            "CREATE TRIGGER fail {target} BEGIN SELECT RAISE(ABORT,'injected'); END"
        ))
        .unwrap();
        let before = dump(&db);
        assert!(merge_import(&mut db, fixture()).is_err());
        assert_eq!(dump(&db), before);
        db.execute_batch("DROP TRIGGER fail").unwrap();
        merge_import(&mut db, fixture()).unwrap();
    }
    for field in ["revision", "generation"] {
        let mut db = empty();
        db.execute_batch(&format!(
            "UPDATE task_checklist_sync_state SET {field}={}",
            protocol::MAX_CLOCK
        ))
        .unwrap();
        let before = dump(&db);
        assert!(merge_import(&mut db, fixture()).is_err());
        assert_eq!(dump(&db), before);
    }
    let mut db = empty();
    merge_import(&mut db, fixture()).unwrap();
    let mut conflict = fixture();
    conflict.task_checklist_items.as_mut().unwrap().items[0].todo_uuid =
        "123e4567-e89b-42d3-a456-426614174009".into();
    conflict.todos[0].title = "must rollback".into();
    conflict.todos[0].updated_at = 9999;
    let before = dump(&db);
    assert!(merge_import(&mut db, conflict).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn checklist_backup_zip_roundtrip_and_integrity() {
    let mut db = empty();
    merge_import(&mut db, fixture()).unwrap();
    let doc = capture_export(&mut db, true, 4000).unwrap();
    let bytes = serde_json::to_vec(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("eggdone-checklist-backup-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("test.eggdone-backup");
    let manifest = BackupManifest {
        format_version: 1,
        created_at: 4000,
        data: backup_manifest_entry("data.json".into(), &bytes),
        assets: vec![],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    write_full_backup_archive(&file, &manifest_bytes, &bytes, &[]).unwrap();
    let verified = read_full_backup_archive(&file, None).unwrap();
    assert_eq!(
        verified.import.task_checklist_items,
        doc.task_checklist_items
    );
    let mut other = empty();
    merge_import(&mut other, verified.import).unwrap();
    assert_eq!(
        store::snapshot(&mut other).unwrap().items,
        doc.task_checklist_items.unwrap()
    );
    let mut altered = bytes.clone();
    altered.push(b' ');
    write_full_backup_archive(&file, &manifest_bytes, &altered, &[]).unwrap();
    assert!(read_full_backup_archive(&file, None).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "invoked by isolated cross-client backup harness"]
fn checklist_backup_cross_client_exchange() {
    let input = std::env::var("EGGDONE_CHECKLIST_BACKUP_INPUT").unwrap();
    let output = std::env::var("EGGDONE_CHECKLIST_BACKUP_OUTPUT").unwrap();
    let mut db = empty();
    merge_import(&mut db, read_import_file(Path::new(&input)).unwrap()).unwrap();
    let exported = capture_export(&mut db, false, 4000).unwrap();
    assert_eq!(
        exported.task_checklist_items,
        fixture().task_checklist_items
    );
    assert_eq!(
        exported.task_checklist_definitions,
        fixture().task_checklist_definitions
    );
    fs::write(output, serde_json::to_vec_pretty(&exported).unwrap()).unwrap();
}
