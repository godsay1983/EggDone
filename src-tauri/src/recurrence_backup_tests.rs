use super::*;
use crate::{
    recurrence_backup, recurrence_protocol, recurrence_store,
    recurrence_transaction::{self, AdvanceRequest, RecurrenceAction},
};

const DEVICE: &str = "00000000-0000-4000-8000-00000000000b";

#[test]
fn shared_harmony_exports_roundtrip_with_desktop_and_continue() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-backup-v2.json"
    ))
    .unwrap();
    for name in ["all_day", "timed"] {
        let incoming: TodoExport = serde_json::from_value(fixtures[name].clone()).unwrap();
        let mut db = empty();
        merge_import(&mut db, incoming).unwrap();
        let exported = serde_json::to_value(export(&db)).unwrap();
        assert_eq!(exported["recurrence"], fixtures[name]["recurrence"]);
        let mut a = exported["todos"].as_array().unwrap().clone();
        let mut b = fixtures[name]["todos"].as_array().unwrap().clone();
        a.sort_by_key(|t| t["uuid"].to_string());
        b.sort_by_key(|t| t["uuid"].to_string());
        assert_eq!(a, b);
        advance(&mut db);
        assert_eq!(
            recurrence_store::snapshot(&db).unwrap().document.rules[0].generated_count,
            3
        );
    }
}
fn empty() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn fixture() -> Connection {
    let mut db = empty();
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let rule: recurrence_protocol::RecurrenceRule =
        serde_json::from_value(value["base_rule"].clone()).unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,repeat_series_uuid,due_date) VALUES(?1,'keep task',0,1,1,?2,?1,?3)", params![rule.first_todo_uuid,DEVICE,rule.current_date]).unwrap();
    recurrence_store::merge(
        &mut db,
        &recurrence_protocol::RecurrenceDocument {
            format_version: 1,
            rules: vec![rule],
        },
    )
    .unwrap();
    db
}
fn export(db: &Connection) -> TodoExport {
    TodoExport {
        format_version: 2,
        exported_at: 4000,
        groups: read_all_groups(db).unwrap(),
        todos: read_all_todos(db).unwrap(),
        notes: vec![],
        note_attachments: vec![],
        attachment_files_included: false,
        recurrence: Some(recurrence_backup::export(db).unwrap()),
    }
}
fn roundtrip(doc: TodoExport) -> TodoExport {
    serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap()
}
fn advance(db: &mut Connection) {
    let rule = recurrence_store::snapshot(db)
        .unwrap()
        .document
        .rules
        .remove(0);
    recurrence_transaction::advance_current(
        db,
        &AdvanceRequest {
            rule_uuid: &rule.uuid,
            current_todo_uuid: &rule.current_todo_uuid,
            action: RecurrenceAction::Complete,
            now: 3000,
            device_id: DEVICE,
            reminder_at: None,
        },
    )
    .unwrap();
}
fn dump(db: &Connection) -> String {
    format!(
        "{:?}{:?}{:?}",
        read_all_todos(db).unwrap(),
        recurrence_store::snapshot(db).unwrap(),
        db.prepare("SELECT key,value FROM app_metadata ORDER BY key")
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    )
}

#[test]
fn v2_rules_and_instances_roundtrip_then_continue_once() {
    let mut source = fixture();
    advance(&mut source);
    let doc = roundtrip(export(&source));
    assert_eq!(doc.recurrence.as_ref().unwrap().instances.len(), 1);
    assert!(!doc.recurrence.as_ref().unwrap().instances[0].purged);
    let mut destination = empty();
    merge_import(&mut destination, doc).unwrap();
    assert_eq!(
        recurrence_store::snapshot(&destination).unwrap().document,
        recurrence_store::snapshot(&source).unwrap().document
    );
    assert_eq!(
        read_all_todos(&destination).unwrap(),
        read_all_todos(&source).unwrap()
    );
    let snapshot = recurrence_store::snapshot(&destination).unwrap();
    assert!(snapshot.revision > snapshot.synced_revision);
    assert!(snapshot.etag.is_none());
    advance(&mut destination);
    assert_eq!(read_all_todos(&destination).unwrap().len(), 3);
    merge_import(&mut destination, roundtrip(export(&source))).unwrap();
    assert_eq!(
        recurrence_store::snapshot(&destination)
            .unwrap()
            .document
            .rules[0]
            .generated_count,
        3
    );
}

#[test]
fn old_backup_keeps_local_rules_and_stopped_tombstone_wins() {
    let mut db = fixture();
    let old = export(&db);
    let mut stopped = recurrence_store::snapshot(&db).unwrap().document;
    stopped.rules[0].deleted_at = Some(2000);
    stopped.rules[0].updated_at = 2000;
    recurrence_store::merge(&mut db, &stopped).unwrap();
    merge_import(&mut db, old).unwrap();
    assert_eq!(recurrence_store::snapshot(&db).unwrap().document, stopped);
    let mut legacy = export(&db);
    legacy.format_version = 1;
    legacy.recurrence = None;
    let before = recurrence_store::snapshot(&db).unwrap();
    merge_import(&mut db, legacy).unwrap();
    let after = recurrence_store::snapshot(&db).unwrap();
    assert_eq!(before.document, after.document);
    assert_eq!(before.revision, after.revision);
}

#[test]
fn immutable_conflict_rolls_back_tasks_and_rules() {
    let mut db = fixture();
    let mut import = export(&db);
    import.todos[0].title = "must roll back".into();
    import.todos[0].updated_at = 9999;
    import.recurrence.as_mut().unwrap().document.rules[0]
        .schedule
        .interval = 2;
    let before = dump(&db);
    assert!(merge_import(&mut db, import).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn legacy_cannot_detach_an_active_rule_to_quick_repeat() {
    let mut db = fixture();
    let mut import = export(&db);
    import.format_version = 1;
    import.recurrence = None;
    import.todos[0].repeat_rule = Some("daily".into());
    import.todos[0].repeat_next_due_date = Some("2026-09-06".into());
    import.todos[0].updated_at = 9999;
    let before = dump(&db);
    assert!(merge_import(&mut db, import).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn missing_source_remains_dormant_without_creating_tasks() {
    let source = fixture();
    let mut import = export(&source);
    import.todos.clear();
    let mut db = empty();
    merge_import(&mut db, import).unwrap();
    assert!(read_all_todos(&db).unwrap().is_empty());
    assert_eq!(
        recurrence_store::snapshot(&db)
            .unwrap()
            .document
            .rules
            .len(),
        1
    );
}

#[test]
fn purged_instance_receipt_survives_export_and_blocks_old_restore() {
    let mut source = fixture();
    advance(&mut source);
    let old = export(&source);
    let id = recurrence_store::snapshot(&source).unwrap().document.rules[0]
        .current_todo_uuid
        .clone();
    source
        .execute("DELETE FROM todos WHERE uuid=?1", [id])
        .unwrap();
    let doc = export(&source);
    assert!(doc.recurrence.as_ref().unwrap().instances[0].purged);
    let mut db = empty();
    merge_import(&mut db, roundtrip(doc)).unwrap();
    let before = dump(&db);
    assert!(merge_import(&mut db, old)
        .unwrap_err()
        .contains("OCCURRENCE_MISSING"));
    assert_eq!(dump(&db), before);
}

#[test]
fn restored_edit_receipts_are_invalidated_but_reminder_receipts_stay_local() {
    let mut db = fixture();
    db.execute_batch("INSERT INTO app_metadata VALUES('recurrence.edit.v1:test','old'); INSERT INTO app_metadata VALUES('reminder.complete.v1:test','old');").unwrap();
    let doc = export(&db);
    let json = serde_json::to_string(&doc).unwrap();
    assert!(!json.contains("recurrence.edit"));
    assert!(!json.contains("reminder.complete"));
    assert!(!json.contains("etag"));
    merge_import(&mut db, doc).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'recurrence.edit.v1:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'reminder.complete.v1:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn invalid_versions_shapes_and_receipts_reject_before_writes() {
    let mut source = fixture();
    advance(&mut source);
    for change in 0..9 {
        let mut raw = serde_json::to_value(export(&source)).unwrap();
        match change {
            0 => {
                raw["format_version"] = 3.into();
            }
            1 => {
                raw.as_object_mut().unwrap().remove("recurrence");
            }
            2 => {
                raw["recurrence"] = serde_json::Value::Null;
            }
            3 => {
                raw["recurrence"]["document"]["rules"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("timezone_id");
            }
            4 => {
                raw["recurrence"]["instances"][0]["todo_uuid"] = "bad".into();
            }
            5 => {
                raw["recurrence"]["instances"][0]["purged"] = true.into();
            }
            6 => {
                raw["recurrence"]["instances"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("planned_due_at");
            }
            7 => {
                raw["recurrence"]["instances"][0]["extra"] = true.into();
            }
            _ => {
                raw["format_version"] = 1.into();
            }
        }
        let mut db = empty();
        let before = dump(&db);
        let result = serde_json::from_value(raw)
            .map_err(|e| e.to_string())
            .and_then(|doc| merge_import(&mut db, doc));
        assert!(result.is_err(), "case {change}");
        assert_eq!(dump(&db), before);
    }
}

#[test]
fn injected_failure_rolls_back_all_domains_and_sync_state() {
    let source = fixture();
    let mut db = empty();
    db.execute_batch("CREATE TRIGGER fail_rule BEFORE INSERT ON recurrence_rules BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    let before = dump(&db);
    assert!(merge_import(&mut db, export(&source)).is_err());
    assert_eq!(dump(&db), before);
}

#[test]
fn v2_archive_and_plain_json_use_same_recurrence_payload() {
    let mut source = fixture();
    advance(&mut source);
    let mut doc = export(&source);
    doc.attachment_files_included = true;
    let bytes = serde_json::to_vec(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("recurrence-backup-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("rules.eggdone-backup");
    let manifest = BackupManifest {
        format_version: 1,
        created_at: 4000,
        data: backup_manifest_entry("data.json".into(), &bytes),
        assets: vec![],
    };
    write_full_backup_archive(&file, &serde_json::to_vec(&manifest).unwrap(), &bytes, &[]).unwrap();
    let validated = read_full_backup_archive(&file, None).unwrap();
    assert_eq!(
        validated.import.recurrence.unwrap().document,
        doc.recurrence.unwrap().document
    );
    fs::remove_dir_all(dir).unwrap();
}
