use crate::task_batch::{self as b, BatchItem, BatchRequest};
use rusqlite::Connection;
use uuid::Uuid;

const BY: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn batch_recovery_reopens_without_duplicates_and_keeps_dirty_revision() {
    let file = std::env::temp_dir().join(format!("eggdone-batch-recovery-{}.db", Uuid::new_v4()));
    let r = request(2);
    {
        let mut db = Connection::open(&file).unwrap();
        crate::db::migrate(&mut db).unwrap();
        b::prepare(&mut db, &r).unwrap();
        assert_eq!(count(&db), 0);
        b::create(&mut db, &r, 100, BY).unwrap();
    }
    let mut db = Connection::open(&file).unwrap();
    crate::db::migrate(&mut db).unwrap();
    assert_eq!(b::pending(&db).unwrap(), Some(r.clone()));
    let revision = dirty(&db);
    b::prepare(&mut db, &r).unwrap();
    b::create(&mut db, &r, 200, BY).unwrap();
    b::forget(&mut db, &r).unwrap();
    assert_eq!(count(&db), 2);
    assert_eq!(dirty(&db), revision);
    assert!(b::pending(&db).unwrap().is_none());
    drop(db);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn batch_recovery_compare_and_clear_never_changes_tasks_or_receipts() {
    let mut db = db();
    let r = request(2);
    b::prepare(&mut db, &r).unwrap();
    let other = request(1);
    assert!(b::prepare(&mut db, &other)
        .unwrap_err()
        .contains("RECOVERY_CONFLICT"));
    assert!(b::forget(&mut db, &other)
        .unwrap_err()
        .contains("RECOVERY_CONFLICT"));
    b::create(&mut db, &r, 100, BY).unwrap();
    db.execute("UPDATE todos SET deleted_at=200", []).unwrap();
    assert!(b::create(&mut db, &r, 300, BY)
        .unwrap_err()
        .contains("STALE"));
    let revision = dirty(&db);
    b::forget(&mut db, &r).unwrap();
    assert_eq!(count(&db), 2);
    assert_eq!(dirty(&db), revision);
    assert!(b::create(&mut db, &r, 400, BY)
        .unwrap_err()
        .contains("STALE"));
}

#[test]
fn batch_recovery_corruption_and_storage_failure_are_not_silenced() {
    let mut db = db();
    let r = request(1);
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('task.batch.pending.v1','broken')",
        [],
    )
    .unwrap();
    assert!(b::pending(&db).unwrap_err().contains("RECOVERY_INVALID"));
    assert!(b::prepare(&mut db, &r).is_err());
    assert!(b::forget(&mut db, &r).is_err());
    assert_eq!(count(&db), 0);
    db.execute(
        "DELETE FROM app_metadata WHERE key='task.batch.pending.v1'",
        [],
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER fail_pending BEFORE INSERT ON app_metadata WHEN NEW.key='task.batch.pending.v1' BEGIN SELECT RAISE(ABORT,'full'); END;").unwrap();
    assert!(b::prepare(&mut db, &r).is_err());
    assert!(b::pending(&db).unwrap().is_none());
    assert_eq!(count(&db), 0);
}
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db
}
fn request(n: usize) -> BatchRequest {
    BatchRequest {
        operation_uuid: Uuid::new_v4().to_string(),
        group_uuid: None,
        items: (0..n)
            .map(|_| BatchItem {
                uuid: Uuid::new_v4().to_string(),
                title: "same title".into(),
            })
            .collect(),
    }
}
fn count(db: &Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0))
        .unwrap()
}
fn dirty(db: &Connection) -> i64 {
    db.query_row(
        "SELECT todos_dirty_version FROM sync_runtime_state",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn batch_limits_and_strict_boundary() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-batch-create-v1.json"
    ))
    .unwrap();
    for c in f["valid"].as_array().unwrap() {
        assert!(b::parse(&c["request"].to_string()).is_ok(), "{}", c["id"]);
    }
    for c in f["invalid"].as_array().unwrap() {
        assert!(b::parse(&c["request"].to_string()).is_err(), "{}", c["id"]);
    }
    for n in [0, 51] {
        assert!(b::validate(&request(n)).is_err());
    }
    let r = request(1);
    assert!(b::parse(
        &serde_json::to_string(&r)
            .unwrap()
            .replace("same title", "\\ud800")
    )
    .is_err());
    assert_eq!(b::parse(&serde_json::to_string(&r).unwrap()).unwrap(), r);
    let mut raw = serde_json::to_value(&r).unwrap();
    raw.as_object_mut().unwrap().remove("group_uuid");
    assert!(b::parse(&raw.to_string()).is_err());
    for title in [
        "",
        " leading",
        "trailing ",
        "line\nbreak",
        "\u{feff}title",
        "\u{202e}bidi",
    ] {
        let mut bad = r.clone();
        bad.items[0].title = title.into();
        assert!(b::validate(&bad).is_err());
    }
    let mut astral = r.clone();
    astral.items[0].title = "\u{1f600}".repeat(50);
    assert!(b::validate(&astral).is_ok());
    astral.items[0].title.push('a');
    assert!(b::validate(&astral).is_err());
    let mut duplicate = request(2);
    duplicate.items[1].uuid = duplicate.items[0].uuid.clone();
    assert!(b::validate(&duplicate).is_err());
    raw = serde_json::to_value(&r).unwrap();
    raw["items"][0]["extra"] = true.into();
    assert!(b::parse(&raw.to_string()).is_err());
}

#[test]
fn batch_preserves_order_duplicates_defaults_and_retry() {
    for n in [1, 50] {
        let mut db = db();
        let r = request(n);
        let saved = b::create(&mut db, &r, 100, BY).unwrap();
        assert_eq!(count(&db), n as i64);
        let ids = db
            .prepare("SELECT uuid FROM todos ORDER BY sort_order")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ids, saved.task_uuids);
        let before = dirty(&db);
        assert_eq!(b::create(&mut db, &r, 200, BY).unwrap(), saved);
        assert_eq!(dirty(&db), before);
        assert_eq!(count(&db), n as i64);
        let mut changed = r.clone();
        changed.items[0].title = "changed".into();
        assert_eq!(
            b::create(&mut db, &changed, 200, BY).unwrap_err(),
            "BATCH_OPERATION_REUSED"
        );
        let blank: i64 = db.query_row("SELECT COUNT(*) FROM todos WHERE note='' AND completed=0 AND pinned=0 AND priority=0 AND due_date IS NULL AND due_at IS NULL AND reminder_at IS NULL AND repeat_rule IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(blank, n as i64);
    }
}

#[test]
fn batch_failures_roll_back_rows_receipt_and_dirty_markers() {
    for sql in [
        "CREATE TRIGGER fail_batch BEFORE INSERT ON todos BEGIN SELECT RAISE(ABORT,'test failure'); END",
        "CREATE TRIGGER fail_batch BEFORE INSERT ON todos WHEN (SELECT COUNT(*) FROM todos)=1 BEGIN SELECT RAISE(ABORT,'test failure'); END",
        "CREATE TRIGGER fail_batch BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'task.batch.create.v1:%' BEGIN SELECT RAISE(ABORT,'test failure'); END",
    ] {
        let mut db = db(); let r = request(3); let before = dirty(&db);
        db.execute_batch(sql).unwrap();
        assert!(b::create(&mut db, &r, 100, BY).unwrap_err().contains("test failure"));
        assert_eq!(count(&db), 0); assert_eq!(dirty(&db), before);
        let receipts: i64 = db.query_row("SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'task.batch.create.v1:%'", [], |r| r.get(0)).unwrap();
        assert_eq!(receipts, 0);
        db.execute_batch("DROP TRIGGER fail_batch").unwrap();
        assert!(b::create(&mut db, &r, 100, BY).is_ok());
    }
}

#[test]
fn batch_stale_receipts_never_resurrect_or_overwrite() {
    for change in [
        "UPDATE todos SET title='edited'",
        "UPDATE todos SET completed=1",
        "UPDATE todos SET deleted_at=200",
        "UPDATE todos SET archived_at=200",
        "UPDATE todos SET sort_order=1234",
        "DELETE FROM todos",
    ] {
        let mut db = db();
        let r = request(2);
        b::create(&mut db, &r, 100, BY).unwrap();
        db.execute_batch(change).unwrap();
        let before = dirty(&db);
        let size = count(&db);
        assert_eq!(
            b::create(&mut db, &r, 300, BY).unwrap_err(),
            "BATCH_STALE_RECEIPT"
        );
        assert_eq!(count(&db), size);
        assert_eq!(dirty(&db), before);
    }
}

#[test]
fn batch_group_collision_and_order_guard() {
    let mut db = db();
    let mut r = request(2);
    r.group_uuid = Some(Uuid::new_v4().to_string());
    assert_eq!(
        b::create(&mut db, &r, 100, BY).unwrap_err(),
        "BATCH_GROUP_MISSING"
    );
    r.group_uuid = None;
    b::create(&mut db, &r, 100, BY).unwrap();
    let mut other = r.clone();
    other.operation_uuid = Uuid::new_v4().to_string();
    assert_eq!(
        b::create(&mut db, &other, 200, BY).unwrap_err(),
        "BATCH_IDENTITY_EXISTS"
    );
    db.execute_batch("UPDATE todos SET sort_order=-9007199254740991")
        .unwrap();
    assert_eq!(
        b::create(&mut db, &request(1), 200, BY).unwrap_err(),
        "BATCH_ORDER_OVERFLOW"
    );
    assert!(b::create(&mut db, &request(1), -1, BY).is_err());
    assert!(b::create(&mut db, &request(1), 100, "bad-device").is_err());
}

#[test]
fn batch_receipt_survives_reopen_and_migration() {
    let path = std::env::temp_dir().join(format!("eggdone-batch-{}.sqlite", Uuid::new_v4()));
    let r = request(2);
    let saved = {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        b::create(&mut db, &r, 100, BY).unwrap()
    };
    let mut db = Connection::open(&path).unwrap();
    crate::db::migrate(&mut db).unwrap();
    assert_eq!(b::create(&mut db, &r, 200, BY).unwrap(), saved);
    assert_eq!(count(&db), 2);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn batch_group_deleted_receipt_and_corrupt_receipt() {
    let mut db = db();
    let mut r = request(2);
    let group = Uuid::new_v4().to_string();
    db.execute("INSERT INTO groups(uuid,name,color,sort_order,created_at,updated_at,updated_by) VALUES(?1,'group','#ffffff',0,1,1,?2)", [&group, BY]).unwrap();
    r.group_uuid = Some(group.clone());
    b::create(&mut db, &r, 100, BY).unwrap();
    let assigned: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE group_uuid=?1",
            [&group],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(assigned, 2);
    db.execute("UPDATE groups SET deleted_at=200 WHERE uuid=?1", [&group])
        .unwrap();
    assert_eq!(
        b::create(&mut db, &r, 300, BY).unwrap_err(),
        "BATCH_STALE_RECEIPT"
    );
    let mut fresh = request(1);
    fresh.group_uuid = Some(group);
    assert_eq!(
        b::create(&mut db, &fresh, 300, BY).unwrap_err(),
        "BATCH_GROUP_MISSING"
    );
    db.execute(
        "UPDATE app_metadata SET value='{}' WHERE key=?1",
        [format!("task.batch.create.v1:{}", r.operation_uuid)],
    )
    .unwrap();
    assert_eq!(
        b::create(&mut db, &r, 300, BY).unwrap_err(),
        "BATCH_INVALID_RECEIPT"
    );
    assert_eq!(count(&db), 2);
}

#[test]
fn batch_orphan_link_rejects_new_identity_and_invalidates_retry() {
    for existing in [false, true] {
        let mut db = db();
        let r = request(2);
        if existing {
            b::create(&mut db, &r, 100, BY).unwrap();
        }
        db.execute("INSERT INTO task_note_links(uuid,todo_uuid,note_uuid,active,record_json) VALUES(?1,?2,?3,1,'{}')",
            [Uuid::new_v4().to_string(), r.items[0].uuid.clone(), Uuid::new_v4().to_string()]).unwrap();
        let before = dirty(&db);
        assert_eq!(
            b::create(&mut db, &r, 200, BY).unwrap_err(),
            if existing {
                "BATCH_STALE_RECEIPT"
            } else {
                "BATCH_IDENTITY_EXISTS"
            }
        );
        assert_eq!(count(&db), if existing { 2 } else { 0 });
        assert_eq!(dirty(&db), before);
    }
}
