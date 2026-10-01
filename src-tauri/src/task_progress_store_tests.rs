use super::*;
use uuid::Uuid;

pub(super) fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&c).unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
pub(super) fn task(c: &Connection) -> String {
    let id = Uuid::new_v4().to_string();
    seed_task(c, &id);
    id
}
fn seed_task(c: &Connection, id: &str) {
    c.execute("INSERT INTO todos(uuid,title,completed,created_at,updated_at,updated_by,sort_order) VALUES(?1,'task',0,1,1,'device',7)",[id]).unwrap();
}
pub(super) fn request(task: &str) -> ProgressWrite {
    ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: task.into(),
        record_uuid: Uuid::new_v4().to_string(),
        action: "create".into(),
        body: "  private progress\nline two  ".into(),
        expected_record: None,
    }
}
pub(super) fn change(view: &ProgressView, action: &str) -> ProgressWrite {
    ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: view.record.task_uuid.clone(),
        record_uuid: view.record.uuid.clone(),
        action: action.into(),
        body: if action == "delete" {
            String::new()
        } else {
            "edited".into()
        },
        expected_record: Some(view.token.clone()),
    }
}
fn n(c: &Connection, table: &str) -> i64 {
    c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn migration_from_26_is_idempotent_and_does_not_touch_parent() {
    let mut c = db();
    let id = task(&c);
    crate::db::remove_task_progress_schema_for_test(&c);
    crate::db::migrate(&mut c).unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(n(&c, "schema_migrations"), 27);
    assert_eq!(n(&c, "task_progress_entries"), 0);
    assert_eq!(
        c.query_row("SELECT updated_at FROM todos WHERE uuid=?1", [id], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(snapshot(&mut c).unwrap().revision, 0);
}

#[test]
fn atomic_receipt_and_normalized_retry_survive_archive() {
    let mut c = db();
    let id = task(&c);
    let mut r = request(&id);
    c.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON task_progress_operations BEGIN SELECT RAISE(ABORT,'failure'); END").unwrap();
    assert_eq!(
        write(&mut c, &r, 100, "device").unwrap_err(),
        "PROGRESS_DATABASE"
    );
    assert_eq!(snapshot(&mut c).unwrap().revision, 0);
    assert_eq!(n(&c, "task_progress_entries"), 0);
    c.execute_batch("DROP TRIGGER fail_receipt").unwrap();
    let first = write(&mut c, &r, 100, "device").unwrap();
    assert_eq!(first.entries[0].record.body, "private progress\nline two");
    let state = snapshot(&mut c).unwrap();
    c.execute("UPDATE todos SET archived_at=200 WHERE uuid=?1", [&id])
        .unwrap();
    let retry = write(&mut c, &r, 1, "other").unwrap();
    assert!(retry.read_only);
    assert_eq!(retry.entries, first.entries);
    assert_eq!(snapshot(&mut c).unwrap(), state);
    r.body.push('x');
    assert_eq!(
        write(&mut c, &r, 100, "device").unwrap_err(),
        "PROGRESS_CONFLICT"
    );
    assert_eq!(n(&c, "task_progress_operations"), 1);
    let columns: Vec<String> = c
        .prepare("PRAGMA table_info(task_progress_operations)")
        .unwrap()
        .query_map([], |r| r.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|s| s == "body"));
}

#[test]
fn committed_operation_survives_database_reopen_without_replay() {
    let path = std::env::temp_dir().join(format!("eggdone-progress-{}.sqlite", Uuid::new_v4()));
    let mut c = Connection::open(&path).unwrap();
    crate::db::migrate(&mut c).unwrap();
    let id = task(&c);
    let r = request(&id);
    let first = write(&mut c, &r, 100, "device").unwrap();
    drop(c);
    let mut reopened = Connection::open(&path).unwrap();
    crate::db::migrate(&mut reopened).unwrap();
    assert_eq!(
        write(&mut reopened, &r, 200, "other-device").unwrap(),
        first
    );
    assert_eq!(n(&reopened, "task_progress_operations"), 1);
    assert_eq!(snapshot(&mut reopened).unwrap().revision, 1);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn completion_allowed_archive_and_trash_readonly_parent_untouched() {
    let mut c = db();
    let id = task(&c);
    c.execute(
        "UPDATE todos SET completed=1,completed_at=20 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    let before: (i64, i64, i64) = c
        .query_row(
            "SELECT completed,updated_at,sort_order FROM todos WHERE uuid=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let first = write(&mut c, &request(&id), 100, "device").unwrap();
    let edit = change(&first.entries[0], "edit");
    write(&mut c, &edit, 90, "device").unwrap();
    assert_eq!(
        before,
        c.query_row(
            "SELECT completed,updated_at,sort_order FROM todos WHERE uuid=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        )
        .unwrap()
    );
    for field in ["archived_at", "deleted_at"] {
        c.execute(
            &format!("UPDATE todos SET {field}=200 WHERE uuid=?1"),
            [&id],
        )
        .unwrap();
        assert!(list(&mut c, &id, None).unwrap().read_only);
        assert_eq!(
            write(&mut c, &request(&id), 200, "device").unwrap_err(),
            "PROGRESS_READ_ONLY"
        );
        c.execute(
            &format!("UPDATE todos SET {field}=NULL WHERE uuid=?1"),
            [&id],
        )
        .unwrap();
    }
    assert!(!list(&mut c, &id, None).unwrap().read_only);
}

#[test]
fn validation_utf16_clock_and_expected_record_are_enforced() {
    let mut c = db();
    let id = task(&c);
    let mut r = request(&id);
    for body in [" \u{feff}\n", "control\u{7f}", "bidi\u{202e}"] {
        r.body = body.into();
        assert_eq!(
            write(&mut c, &r, 100, "device").unwrap_err(),
            "PROGRESS_INVALID"
        );
    }
    r.body = "\u{1f600}".repeat(501);
    assert_eq!(
        write(&mut c, &r, 100, "device").unwrap_err(),
        "PROGRESS_LIMIT"
    );
    r.body = "\u{1f600}".repeat(500);
    let first = write(&mut c, &r, 100, "device").unwrap();
    let edit = change(&first.entries[0], "edit");
    let edited = write(&mut c, &edit, 1, "device").unwrap();
    assert_eq!(edited.entries[0].record.updated_at, 100);
    assert_eq!(edited.entries[0].record.created_at, 100);
    assert_eq!(edited.entries[0].record.clock, 2);
    assert_eq!(
        write(&mut c, &change(&first.entries[0], "edit"), 101, "device").unwrap_err(),
        "PROGRESS_CONFLICT"
    );
    c.execute(
        "UPDATE task_progress_entries SET clock=?1",
        [protocol::MAX_CLOCK],
    )
    .unwrap();
    let page = list(&mut c, &id, None).unwrap();
    assert_eq!(
        write(&mut c, &change(&page.entries[0], "edit"), 100, "device").unwrap_err(),
        "PROGRESS_LIMIT"
    );
    assert_eq!(n(&c, "task_progress_operations"), 2);
}

#[test]
fn thirty_row_keyset_order_and_grouped_counts_exclude_tombstones() {
    let mut c = db();
    let id = task(&c);
    for _ in 0..65 {
        write(&mut c, &request(&id), 100, "device").unwrap();
    }
    let first = list(&mut c, &id, None).unwrap();
    assert_eq!(first.entries.len(), 30);
    assert_eq!(first.total, 65);
    let second = list(&mut c, &id, first.next_cursor.as_ref()).unwrap();
    let third = list(&mut c, &id, second.next_cursor.as_ref()).unwrap();
    assert_eq!(second.entries.len(), 30);
    assert_eq!(third.entries.len(), 5);
    assert!(third.next_cursor.is_none());
    let all: Vec<_> = first
        .entries
        .iter()
        .chain(&second.entries)
        .chain(&third.entries)
        .map(|v| v.record.uuid.clone())
        .collect();
    assert!(all.windows(2).all(|p| p[0] > p[1]));
    write(&mut c, &change(&first.entries[0], "edit"), 999, "device").unwrap();
    assert_eq!(
        list(&mut c, &id, None).unwrap().entries[0].record.uuid,
        all[0]
    );
    write(&mut c, &change(&third.entries[0], "delete"), 999, "device").unwrap();
    let absent = Uuid::new_v4().to_string();
    let totals = counts(&c, &[id.clone(), absent]).unwrap();
    assert_eq!(
        totals.iter().map(|t| t.count).collect::<Vec<_>>(),
        vec![64, 0]
    );
    assert_eq!(counts(&c, &vec![id; 20001]).unwrap_err(), "PROGRESS_LIMIT");
}

#[test]
fn orphan_sync_records_retained_but_hidden_until_parent_arrives() {
    let mut c = db();
    let id = Uuid::new_v4().to_string();
    let remote = Document {
        format_version: 1,
        entries: vec![Entry {
            uuid: Uuid::new_v4().to_string(),
            task_uuid: id.clone(),
            body: "remote".into(),
            created_at: 1,
            created_by: "remote".into(),
            updated_at: 1,
            updated_by: "remote".into(),
            clock: 1,
            deleted_at: None,
        }],
    };
    restore(&mut c, &remote).unwrap();
    assert_eq!(snapshot(&mut c).unwrap().document, remote);
    assert_eq!(list(&mut c, &id, None).unwrap_err(), "PROGRESS_UNAVAILABLE");
    assert_eq!(counts(&c, std::slice::from_ref(&id)).unwrap()[0].count, 0);
    seed_task(&c, &id);
    assert_eq!(list(&mut c, &id, None).unwrap().total, 1);
}

#[test]
fn tombstone_clears_body_and_wins_against_late_edit() {
    let mut c = db();
    let id = task(&c);
    let page = write(&mut c, &request(&id), 100, "device").unwrap();
    let mut old = snapshot(&mut c).unwrap().document;
    let r = change(&page.entries[0], "delete");
    assert_eq!(write(&mut c, &r, 90, "device").unwrap().total, 0);
    let tombstone = snapshot(&mut c).unwrap().document.entries[0].clone();
    assert!(tombstone.body.is_empty());
    assert_eq!(tombstone.deleted_at, Some(100));
    old.entries[0].clock = protocol::MAX_CLOCK;
    restore(&mut c, &old).unwrap();
    assert_eq!(snapshot(&mut c).unwrap().document.entries[0], tombstone);
    assert_eq!(
        write(&mut c, &change(&page.entries[0], "edit"), 101, "device").unwrap_err(),
        "PROGRESS_DELETED"
    );
    assert_eq!(write(&mut c, &r, 101, "device").unwrap().total, 0);
}

#[test]
fn physical_delete_and_terminal_sanitize_all_local_copies_and_reject_replay() {
    for terminal in [false, true] {
        let mut c = db();
        let id = task(&c);
        let r = request(&id);
        write(&mut c, &r, 100, "device").unwrap();
        let mut remote = snapshot(&mut c).unwrap().document;
        c.execute(
            "INSERT INTO task_progress_notices(task_uuid) VALUES(?1)",
            [&id],
        )
        .unwrap();
        let revision = snapshot(&mut c).unwrap().revision;
        if terminal {
            c.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,200)",params![id,Uuid::new_v4().to_string()]).unwrap();
        } else {
            c.execute("DELETE FROM todos WHERE uuid=?1", [&id]).unwrap();
        }
        for table in [
            "task_progress_entries",
            "task_progress_operations",
            "task_progress_notices",
        ] {
            assert_eq!(n(&c, table), 0);
        }
        assert!(snapshot(&mut c).unwrap().revision > revision);
        assert_eq!(
            write(&mut c, &r, 100, "device").unwrap_err(),
            "PROGRESS_UNAVAILABLE"
        );
        if terminal {
            // Terminal dominates even a conflicting immutable identity in a late backup.
            remote.entries[0].created_by = "another".into();
            restore(&mut c, &remote).unwrap();
            assert!(snapshot(&mut c).unwrap().document.entries.is_empty());
            assert!(c.execute("INSERT INTO task_progress_entries SELECT ?1,?2,'body',1,'remote',1,'remote',1,NULL",params![r.record_uuid,id]).is_err());
        }
    }
}

#[test]
fn revision_overflow_rolls_back_crud_merge_and_purge() {
    let mut c = db();
    let id = task(&c);
    let r = request(&id);
    write(&mut c, &r, 100, "device").unwrap();
    c.execute(
        "UPDATE task_progress_sync_state SET revision=?1",
        [protocol::MAX_CLOCK],
    )
    .unwrap();
    let before = snapshot(&mut c).unwrap();
    let page = list(&mut c, &id, None).unwrap();
    assert_eq!(
        write(&mut c, &change(&page.entries[0], "edit"), 100, "device").unwrap_err(),
        "PROGRESS_LIMIT"
    );
    assert_eq!(
        write(&mut c, &request(&id), 100, "device").unwrap_err(),
        "PROGRESS_LIMIT"
    );
    let mut remote = before.document.clone();
    remote.entries[0].clock = 2;
    assert_eq!(restore(&mut c, &remote).unwrap_err(), "PROGRESS_LIMIT");
    assert!(c.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,200)",params![id,Uuid::new_v4().to_string()]).is_err());
    assert!(c.execute("DELETE FROM todos WHERE uuid=?1", [&id]).is_err());
    assert_eq!(snapshot(&mut c).unwrap(), before);
    assert_eq!(n(&c, "task_progress_operations"), 1);
    assert_eq!(n(&c, "task_progress_notices"), 0);
    assert_eq!(n(&c, "lifecycle_terminals"), 0);
}

#[test]
fn conflicting_identity_merge_is_atomic_even_with_an_unrelated_new_row() {
    let mut c = db();
    let id = task(&c);
    write(&mut c, &request(&id), 100, "device").unwrap();
    let before = snapshot(&mut c).unwrap();
    let mut remote = before.document.clone();
    let mut extra = remote.entries[0].clone();
    extra.uuid = Uuid::new_v4().to_string();
    remote.entries.push(extra);
    remote.entries[0].created_by = "other".into();
    assert_eq!(restore(&mut c, &remote).unwrap_err(), "PROGRESS_CONFLICT");
    assert_eq!(snapshot(&mut c).unwrap(), before);
}
