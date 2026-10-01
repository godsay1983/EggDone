use super::*;
use crate::{task_progress_protocol as progress, task_progress_store as store};

const TASK: &str = "123e4567-e89b-42d3-a456-426614174000";
const FIXTURE: &str = include_str!("../../scripts/fixtures/task-progress-backup-v9.json");

fn fixture() -> TodoExport {
    serde_json::from_str(FIXTURE).unwrap()
}
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn dump(db: &Connection) -> Vec<String> {
    [
        "todos",
        "notes",
        "recurrence_rules",
        "task_note_links",
        "task_checklist_items",
        "task_checklist_definitions",
        "task_templates",
        "lifecycle_terminals",
        "daily_plans",
        "daily_plan_events",
        "daily_plan_completions",
        "task_workflow_states",
        "task_progress_entries",
        "task_progress_operations",
        "task_progress_notices",
        "task_progress_sync_state",
        "sync_runtime_state",
        "app_metadata",
    ]
    .iter()
    .map(|table| {
        let mut query = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = query.column_count();
        let rows = query
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
fn v9_always_exports_progress_and_counts_records_not_tasks() {
    let mut source = db();
    let empty = capture_export(&mut source, false, 1).unwrap();
    assert_eq!(empty.format_version, 9);
    assert_eq!(empty.task_progress, Some(progress::Document::default()));
    assert_eq!(empty.extra["lifecycle_terminals"], serde_json::json!([]));
    let preview = build_preview(&source, Path::new("v9.json"), &fixture()).unwrap();
    assert_eq!(
        (
            preview.progress_total,
            preview.progress_added,
            preview.progress_deleted
        ),
        (3, 2, 1)
    );
    assert!(preview.progress_metadata_included);
    merge_import(&mut source, fixture()).unwrap();
    let preview = build_preview(&source, Path::new("v9.json"), &fixture()).unwrap();
    assert_eq!(preview.progress_unchanged, 3);
    let export = capture_export(&mut source, false, 1000).unwrap();
    let mut target = db();
    merge_import(
        &mut target,
        serde_json::from_str(&serde_json::to_string(&export).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        store::read_in_transaction(&target).unwrap().document,
        fixture().task_progress.unwrap()
    );
    let full = capture_export(&mut source, true, 1000).unwrap();
    assert_eq!(full.format_version, 9);
    assert_eq!(full.task_progress, export.task_progress);
    assert_eq!(BACKUP_FORMAT_VERSION, 1);
    let page = store::list(&mut target, TASK, None).unwrap();
    assert_eq!(page.total, 1); // The other live record has an unknown parent, not a deleted one.
}

#[test]
fn full_backup_keeps_outer_version_one_with_v9_progress_inner_document() {
    let mut source = db();
    merge_import(&mut source, fixture()).unwrap();
    let document = capture_export(&mut source, true, 1000).unwrap();
    let data = serde_json::to_vec(&document).unwrap();
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        created_at: 1000,
        data: backup_manifest_entry("data.json".into(), &data),
        assets: Vec::new(),
    };
    assert_eq!(manifest.format_version, 1);
    let directory =
        std::env::temp_dir().join(format!("eggdone-progress-backup-{}", Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("isolated.eggdone-backup");
    write_full_backup_archive(&path, &serde_json::to_vec(&manifest).unwrap(), &data, &[]).unwrap();
    let restored = read_full_backup_archive(&path, None).unwrap();
    assert_eq!(restored.manifest.format_version, 1);
    assert_eq!(restored.import.format_version, 9);
    assert_eq!(restored.import.task_progress, document.task_progress);
    let mut target = db();
    merge_import(&mut target, restored.import).unwrap();
    assert_eq!(
        store::read_in_transaction(&target).unwrap().document,
        fixture().task_progress.unwrap()
    );
    fs::remove_file(path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn versions_one_through_eight_omit_not_delete_and_reject_new_domain() {
    let mut target = db();
    merge_import(&mut target, fixture()).unwrap();
    let expected = store::read_in_transaction(&target).unwrap().document;
    for version in 1..=8 {
        let mut old = fixture();
        old.format_version = version;
        old.task_progress = None;
        if version < 2 {
            old.recurrence = None;
        }
        if version < 3 {
            old.task_note_links = None;
        }
        if version < 4 {
            old.task_checklist_items = None;
            old.task_checklist_definitions = None;
        }
        if version < 5 {
            old.task_templates = None;
        }
        if version < 6 {
            old.extra.remove("lifecycle_terminals");
        }
        if version < 7 {
            old.extra.remove("daily_planning");
        }
        if version < 8 {
            old.extra.remove("task_workflow");
        }
        let mut illegal: TodoExport =
            serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
        illegal.task_progress = fixture().task_progress;
        assert!(validate_import(&illegal).is_err());
        let before = store::read_in_transaction(&target).unwrap();
        merge_import(&mut target, old).unwrap();
        let after = store::read_in_transaction(&target).unwrap();
        assert_eq!(after.document, expected);
        assert!(after.generation > before.generation);
    }
    let mut missing = fixture();
    missing.task_progress = None;
    assert!(validate_import(&missing).is_err());
}

#[test]
fn damaged_v9_and_late_progress_failure_rollback_every_domain() {
    let mut target = db();
    merge_import(&mut target, fixture()).unwrap();
    let original = dump(&target);
    let mut conflict = fixture();
    conflict.todos[0].title = "must rollback".into();
    conflict.todos[0].updated_at += 100;
    conflict.task_progress.as_mut().unwrap().entries[0].created_by = "different-identity".into();
    assert!(merge_import(&mut target, conflict).is_err());
    assert_eq!(dump(&target), original);
    let raw = serde_json::to_string(&fixture()).unwrap();
    for damaged in [
        raw.replace("\"clock\":2", "\"clock\":2.00000000000000000001"),
        raw.replace("\"clock\":2", "\"clock\":2,\"clock\":2"),
        raw.replace("\"deleted_at\":null", "\"unknown\":null"),
    ] {
        assert!(serde_json::from_str::<TodoExport>(&damaged).is_err());
        assert_eq!(dump(&target), original);
    }
    target.execute_batch("CREATE TRIGGER fail_progress_import BEFORE UPDATE ON task_progress_sync_state BEGIN SELECT RAISE(ABORT,'injected failure'); END").unwrap();
    let mut changed = fixture();
    changed.todos[0].title = "must rollback".into();
    changed.todos[0].updated_at += 100;
    assert!(merge_import(&mut target, changed).is_err());
    assert_eq!(dump(&target), original);
}

#[test]
fn import_invalidates_retry_notices_generation_but_does_not_recover_ack() {
    let mut target = db();
    merge_import(&mut target, fixture()).unwrap();
    store::write(
        &mut target,
        &store::ProgressWrite {
            operation_uuid: Uuid::new_v4().to_string(),
            task_uuid: TASK.into(),
            record_uuid: Uuid::new_v4().to_string(),
            action: "create".into(),
            body: "local unacknowledged edit".into(),
            expected_record: None,
        },
        50,
        "local",
    )
    .unwrap();
    target
        .execute(
            "INSERT INTO task_progress_notices(task_uuid) VALUES(?1)",
            [TASK],
        )
        .unwrap();
    target
        .execute(
            "UPDATE task_progress_sync_state SET etag='local-target' WHERE id=1",
            [],
        )
        .unwrap();
    let before = store::read_in_transaction(&target).unwrap();
    let backup = capture_export(&mut target, false, 100).unwrap();
    let raw = serde_json::to_string(&backup).unwrap();
    for forbidden in [
        "request_digest",
        "committed_token",
        "synced_revision",
        "etag",
        "generation",
        "published",
    ] {
        assert!(!raw.contains(forbidden));
    }
    merge_import(&mut target, backup).unwrap();
    let after = store::read_in_transaction(&target).unwrap();
    assert_eq!(after.document, before.document);
    assert_eq!(after.synced_revision, before.synced_revision);
    assert_eq!(after.etag.as_deref(), Some("local-target"));
    assert!(after.generation > before.generation);
    for table in ["task_progress_operations", "task_progress_notices"] {
        assert_eq!(
            target
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn progress_purge_fingerprint_late_write_and_terminal_precedence() {
    let mut target = db();
    merge_import(&mut target, fixture()).unwrap();
    target
        .execute("UPDATE todos SET deleted_at=30 WHERE uuid=?1", [TASK])
        .unwrap();
    let plan = crate::purge::prepare(
        &mut target,
        Some(vec![crate::purge::Target {
            kind: "todo".into(),
            uuid: TASK.into(),
        }]),
        40,
    )
    .unwrap();
    let mut late = fixture().task_progress.unwrap();
    late.entries[0].body = "late remote edit".into();
    late.entries[0].clock += 1;
    store::restore(&mut target, &late).unwrap();
    let result = crate::purge::execute_batch(&mut target, &plan.operation_uuid, 50).unwrap();
    assert_eq!((result.purged, result.skipped), (0, 1));
    let plan = crate::purge::prepare(
        &mut target,
        Some(vec![crate::purge::Target {
            kind: "todo".into(),
            uuid: TASK.into(),
        }]),
        60,
    )
    .unwrap();
    assert_eq!(
        crate::purge::execute_batch(&mut target, &plan.operation_uuid, 70)
            .unwrap()
            .purged,
        1
    );
    assert!(store::read_in_transaction(&target)
        .unwrap()
        .document
        .entries
        .iter()
        .all(|e| e.task_uuid != TASK));
    store::restore(&mut target, &late).unwrap();
    assert!(store::read_in_transaction(&target)
        .unwrap()
        .document
        .entries
        .iter()
        .all(|e| e.task_uuid != TASK));
    let mut only_progress = capture_export(&mut target, false, 100).unwrap();
    only_progress.task_progress = Some(late);
    merge_import(&mut target, only_progress).unwrap();
    assert!(store::read_in_transaction(&target)
        .unwrap()
        .document
        .entries
        .iter()
        .all(|e| e.task_uuid != TASK));
}

#[test]
fn progress_dirty_domain_keeps_purge_pending_after_lifecycle_ack() {
    let mut target = db();
    merge_import(&mut target, fixture()).unwrap();
    target.execute("UPDATE sync_settings SET endpoint='https://example.invalid',bucket='isolated-test' WHERE id=1", []).unwrap();
    target
        .execute("UPDATE todos SET deleted_at=30 WHERE uuid=?1", [TASK])
        .unwrap();
    let plan = crate::purge::prepare(
        &mut target,
        Some(vec![crate::purge::Target {
            kind: "todo".into(),
            uuid: TASK.into(),
        }]),
        40,
    )
    .unwrap();
    crate::purge::execute_batch(&mut target, &plan.operation_uuid, 50).unwrap();
    target.execute_batch("UPDATE lifecycle_sync_state SET synced_revision=revision,etag='ledger-ack'; UPDATE sync_runtime_state SET last_result='success',dirty_domains='[]';").unwrap();
    assert!(crate::sync_runtime_state::get_snapshot(&target)
        .unwrap()
        .dirty_domains
        .contains(&"progress".into()));
    assert!(
        crate::purge::status(&target, &plan.operation_uuid)
            .unwrap()
            .sync_pending
    );
    assert!(crate::purge::unfinished(&target).unwrap().is_some());
    target
        .execute(
            "UPDATE task_progress_sync_state SET synced_revision=revision WHERE id=1",
            [],
        )
        .unwrap();
    assert!(
        !crate::purge::status(&target, &plan.operation_uuid)
            .unwrap()
            .sync_pending
    );
    assert!(crate::purge::unfinished(&target).unwrap().is_none());
}

#[test]
#[ignore = "Run by the isolated Harmony harness using the parent-built test binary"]
fn task_progress_backup_cross_client_exchange() {
    let mut target = db();
    let incoming = match std::env::var("EGGDONE_PROGRESS_BACKUP_INPUT") {
        Ok(path) => read_import_file(Path::new(&path)).unwrap(),
        Err(_) => fixture(),
    };
    merge_import(&mut target, incoming).unwrap();
    let export = capture_export(&mut target, false, 1000).unwrap();
    assert_eq!(export.format_version, 9);
    let output = std::env::var("EGGDONE_PROGRESS_BACKUP_OUTPUT").unwrap();
    fs::write(output, serde_json::to_vec(&export).unwrap()).unwrap();
}
