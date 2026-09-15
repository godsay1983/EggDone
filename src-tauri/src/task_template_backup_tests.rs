use super::*;
use crate::task_template_store as store;
fn fixture() -> TodoExport {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/task-template-backup-v5.json"
    ))
    .unwrap()
}
fn empty() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
#[test]
fn template_backup_v5_preview_roundtrip_legacy_and_invalid_input() {
    let mut db = empty();
    merge_import(&mut db, fixture()).unwrap();
    let exported = capture_export(&mut db, false, 4000).unwrap();
    assert_eq!(exported.format_version, 5);
    assert_eq!(exported.task_templates, fixture().task_templates);
    let preview = build_preview(&db, Path::new("v5.json"), &exported).unwrap();
    assert_eq!(
        (
            preview.template_total,
            preview.template_deleted,
            preview.template_metadata_included
        ),
        (2, 1, true)
    );
    let state = store::snapshot(&mut db).unwrap();
    let legacy: TodoExport = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-checklist-backup-v4.json"
    ))
    .unwrap();
    merge_import(&mut db, legacy).unwrap();
    assert_eq!(store::snapshot(&mut db).unwrap(), state);
    for kind in 0..5 {
        let mut raw = serde_json::to_value(fixture()).unwrap();
        match kind {
            0 => {
                raw.as_object_mut().unwrap().remove("task_templates");
            }
            1 => raw["task_templates"] = serde_json::Value::Null,
            2 => raw["format_version"] = 4.into(),
            3 => raw["task_templates"]["templates"][0]["content"]["extra"] = true.into(),
            _ => raw["task_templates"]["format_version"] = 2.into(),
        }
        let result = serde_json::from_value::<TodoExport>(raw)
            .map_err(|_| "parse".into())
            .and_then(|doc| validate_import(&doc));
        assert!(result.is_err(), "kind {kind}");
        assert_eq!(store::snapshot(&mut db).unwrap(), state);
    }
}
#[test]
fn template_restore_failure_rolls_back_other_domains_and_invalidates_receipts() {
    let mut db = empty();
    db.execute_batch("CREATE TRIGGER fail_template_restore BEFORE INSERT ON task_templates BEGIN SELECT RAISE(ABORT,'restore'); END;").unwrap();
    assert!(merge_import(&mut db, fixture()).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_template_restore")
        .unwrap();
    merge_import(&mut db, fixture()).unwrap();
    let row = fixture()
        .task_templates
        .unwrap()
        .templates
        .into_iter()
        .find(|t| t.deleted_at.is_none())
        .unwrap();
    let mut content = row.content.clone();
    content.name = "Changed".into();
    let request = store::TemplateWrite {
        operation_uuid: uuid::Uuid::new_v4().to_string(),
        uuid: row.uuid.clone(),
        expected: Some(row),
        content,
        deleted: false,
    };
    store::save(&mut db, &request, 5000, "device").unwrap();
    let doc = capture_export(&mut db, false, 6000).unwrap();
    merge_import(&mut db, doc).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM task_template_operations", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    assert!(store::save(&mut db, &request, 7000, "device").is_err());
}
#[test]
#[ignore = "invoked by isolated cross-client template backup harness"]
fn template_backup_cross_client_exchange() {
    let input = std::env::var("EGGDONE_TEMPLATE_BACKUP_INPUT").unwrap();
    let output = std::env::var("EGGDONE_TEMPLATE_BACKUP_OUTPUT").unwrap();
    let mut db = empty();
    merge_import(&mut db, read_import_file(Path::new(&input)).unwrap()).unwrap();
    let exported = capture_export(&mut db, false, 4000).unwrap();
    assert_eq!(exported.task_templates, fixture().task_templates);
    fs::write(output, serde_json::to_vec_pretty(&exported).unwrap()).unwrap();
}
