use super::*;
use protocol::{LinkDocument, TaskNoteLink};

const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
fn incoming() -> LinkDocument {
    LinkDocument {
        format_version: 1,
        links: vec![TaskNoteLink {
            uuid: protocol::link_uuid(TODO, NOTE).unwrap(),
            todo_uuid: TODO.into(),
            note_uuid: NOTE.into(),
            created_at: 10,
            updated_at: 100,
            updated_by: "remote".into(),
            deleted_at: None,
        }],
    }
}
fn empty() -> LinkDocument {
    LinkDocument {
        format_version: 1,
        links: vec![],
    }
}
fn fixture() -> (Connection, String) {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'todo',0,1,1,'a')", [TODO]).unwrap();
    db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'note','body','default',0,1,1,'a')", [NOTE]).unwrap();
    db.execute_batch("UPDATE todos SET updated_by='00000000-0000-4000-8000-00000000000b'; UPDATE notes SET updated_by='00000000-0000-4000-8000-00000000000b'").unwrap();
    let epoch = sync_target::capture(&db).unwrap();
    (db, epoch)
}
fn state(db: &Connection) -> (i64, i64, Option<String>) {
    db.query_row(
        "SELECT revision,synced_revision,etag FROM task_note_link_sync_state",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap()
}

#[test]
fn atomic_payloads_and_ack_do_not_ack_entity_domains() {
    let (mut db, epoch) = fixture();
    let before: String = db
        .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
            r.get(0)
        })
        .unwrap();
    let snapshot = prepare(&mut db, &epoch, &incoming(), 200).unwrap();
    assert!(snapshot.todo_json.contains("todo"));
    assert!(snapshot.note_json.contains("body"));
    assert_eq!(
        protocol::parse_document(&snapshot.links_json).unwrap(),
        incoming()
    );
    assert_eq!(state(&db), (1, 0, None));
    assert!(is_current(&mut db, &snapshot).unwrap());
    assert!(acknowledge(&mut db, &snapshot, "\"etag\"").unwrap());
    assert_eq!(state(&db), (1, 1, Some("\"etag\"".into())));
    assert!(acknowledge(&mut db, &snapshot, "\"etag\"").unwrap());
    let after: String = db
        .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    for etag in ["", "  ", "bad\n"] {
        assert!(acknowledge(&mut db, &snapshot, etag).is_err());
    }
}

#[test]
fn stale_entity_and_link_uploads_cannot_clear_dirty_or_replace_etag() {
    for sql in [
        "UPDATE todos SET title='changed'",
        "UPDATE notes SET content='changed'",
        "UPDATE sync_runtime_state SET todos_dirty_version=todos_dirty_version+1",
        "UPDATE sync_runtime_state SET notes_dirty_version=notes_dirty_version+1",
    ] {
        let (mut db, epoch) = fixture();
        let snapshot = prepare(&mut db, &epoch, &incoming(), 200).unwrap();
        db.execute_batch(sql).unwrap();
        // A remote merge can have identical revision tokens: payload equality still detects it.
        if sql.contains("SET title") || sql.contains("SET content") {
            db.execute(
                "UPDATE sync_runtime_state SET todos_dirty_version=?1,notes_dirty_version=?2",
                params![snapshot.todo_revision, snapshot.note_revision],
            )
            .unwrap();
        }
        let before = state(&db);
        assert!(!is_current(&mut db, &snapshot).unwrap());
        assert!(!acknowledge(&mut db, &snapshot, "stale").unwrap());
        assert_eq!(state(&db), before);
    }
    let (mut db, epoch) = fixture();
    let snapshot = prepare(&mut db, &epoch, &incoming(), 200).unwrap();
    let mut removed = incoming();
    removed.links[0].updated_at = 300;
    removed.links[0].deleted_at = Some(300);
    let newer = prepare(&mut db, &epoch, &removed, 300).unwrap();
    assert!(acknowledge(&mut db, &newer, "new").unwrap());
    assert!(!acknowledge(&mut db, &snapshot, "old").unwrap());
    assert_eq!(state(&db), (2, 2, Some("new".into())));
}

#[test]
fn entity_tombstones_reconcile_but_absence_archive_and_restore_do_not_relink() {
    for table in ["todos", "notes"] {
        let (mut db, epoch) = fixture();
        db.execute_batch(&format!("UPDATE {table} SET deleted_at=500,updated_at=600"))
            .unwrap();
        let result = prepare(&mut db, &epoch, &incoming(), 50).unwrap();
        let deleted = protocol::parse_document(&result.links_json).unwrap();
        assert_eq!(deleted.links[0].deleted_at, Some(600));
        assert_eq!(deleted.links[0].created_at, 10);
        assert_eq!(prepare(&mut db, &epoch, &incoming(), 50).unwrap(), result);
        db.execute_batch(&format!("UPDATE {table} SET deleted_at=NULL"))
            .unwrap();
        assert_eq!(
            protocol::parse_document(
                &prepare(&mut db, &epoch, &incoming(), 700)
                    .unwrap()
                    .links_json
            )
            .unwrap(),
            deleted
        );
    }
    for sql in [
        "DELETE FROM todos",
        "DELETE FROM notes",
        "UPDATE todos SET archived_at=500",
        "UPDATE todos SET completed=1",
    ] {
        let (mut db, epoch) = fixture();
        db.execute_batch(sql).unwrap();
        let result = prepare(&mut db, &epoch, &incoming(), 200).unwrap();
        assert_eq!(
            protocol::parse_document(&result.links_json).unwrap(),
            incoming()
        );
        assert_eq!(
            protocol::parse_document(&prepare(&mut db, &epoch, &empty(), 200).unwrap().links_json)
                .unwrap(),
            incoming()
        );
    }
}

#[test]
fn target_change_rejects_delayed_work_and_invalidates_links_atomically() {
    let (mut db, epoch) = fixture();
    let snapshot = prepare(&mut db, &epoch, &incoming(), 200).unwrap();
    acknowledge(&mut db, &snapshot, "old").unwrap();
    sync_target::invalidate(&db).unwrap();
    assert_eq!(state(&db), (2, 1, None));
    assert!(!acknowledge(&mut db, &snapshot, "late").unwrap());
    assert_eq!(
        prepare(&mut db, &epoch, &empty(), 200).unwrap_err(),
        "TASK_NOTE_LINK_CONFIG_CHANGED"
    );
    sync_target::activate(&db).unwrap();
    let next = sync_target::capture(&db).unwrap();
    assert_ne!(epoch, next);
    assert!(!acknowledge(&mut db, &snapshot, "late").unwrap());
    let next_snapshot = prepare(&mut db, &next, &empty(), 300).unwrap();
    assert!(acknowledge(&mut db, &next_snapshot, "new").unwrap());
    db.execute(
        "UPDATE task_note_link_sync_state SET revision=9007199254740991",
        [],
    )
    .unwrap();
    assert_eq!(
        sync_target::invalidate(&db).unwrap_err(),
        "SYNC_TARGET_REVISION_LIMIT"
    );
    assert!(sync_target::is_current(&db, &next).unwrap());
    assert_eq!(state(&db).2, Some("new".into()));
}

#[test]
fn preparation_and_ack_failures_roll_back_and_can_retry() {
    let (mut db, epoch) = fixture();
    db.execute_batch(
        "UPDATE todos SET deleted_at=500,updated_at=500;
        CREATE TRIGGER fail BEFORE UPDATE ON task_note_links BEGIN SELECT RAISE(ABORT,'fail'); END",
    )
    .unwrap();
    assert!(prepare(&mut db, &epoch, &incoming(), 600).is_err());
    assert_eq!(state(&db), (0, 0, None));
    assert!(store::snapshot(&db).unwrap().document.links.is_empty());
    db.execute_batch("DROP TRIGGER fail").unwrap();
    let snapshot = prepare(&mut db, &epoch, &incoming(), 600).unwrap();
    db.execute_batch("CREATE TRIGGER fail BEFORE UPDATE OF synced_revision ON task_note_link_sync_state BEGIN SELECT RAISE(ABORT,'fail'); END").unwrap();
    assert!(acknowledge(&mut db, &snapshot, "etag").is_err());
    assert_eq!(state(&db), (2, 0, None));
    db.execute_batch("DROP TRIGGER fail").unwrap();
    assert!(acknowledge(&mut db, &snapshot, "etag").unwrap());
    let mut max = incoming();
    max.links[0].updated_at = protocol::MAX_CLOCK;
    let before = state(&db);
    assert_eq!(
        prepare(&mut db, &epoch, &max, 600).unwrap_err(),
        "TASK_NOTE_LINK_CLOCK_EXHAUSTED"
    );
    assert_eq!(state(&db), before);
    let mut invalid = incoming();
    invalid.links[0].uuid = "bad".into();
    assert!(prepare(&mut db, &epoch, &invalid, 600).is_err());
    assert_eq!(state(&db), before);
    // A database read failure is an error, not an accepted (or silently ignored) receipt.
    db.execute_batch("ALTER TABLE app_metadata RENAME TO unavailable_metadata")
        .unwrap();
    assert!(acknowledge(&mut db, &snapshot, "etag").is_err());
}
