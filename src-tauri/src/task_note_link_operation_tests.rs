use crate::task_note_link_operations::*;
use crate::task_note_link_protocol::*;
use crate::task_note_link_store as store;
use rusqlite::{params, Connection};

const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'source','body','default',0,1,1,'device')",[NOTE]).unwrap();
    db
}
fn draft() -> LinkedTodoDraft {
    LinkedTodoDraft {
        todo_uuid: TODO.into(),
        note_uuid: NOTE.into(),
        title: "linked task".into(),
        note: "details".into(),
        group_uuid: None,
        due_date: Some("2026-09-13".into()),
        due_at: None,
        reminder_at: Some(1790000000000),
        priority: 1,
    }
}
fn scalar(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn link_commands_are_idempotent_and_stale_requests_do_not_reactivate() {
    let mut db = fixture();
    let a = create(&mut db, &draft(), 100, "a").unwrap();
    assert_eq!(create(&mut db, &draft(), 200, "a").unwrap(), a);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM todos"), 1);
    let d = change(&mut db, TODO, NOTE, false, Some(&a), 10, "a")
        .unwrap()
        .unwrap();
    assert!(d.updated_at > a.updated_at);
    assert_eq!(create(&mut db, &draft(), 300, "a").unwrap(), d);
    assert!(change(&mut db, TODO, NOTE, true, None, 400, "a").is_err());
    let b = change(&mut db, TODO, NOTE, true, Some(&d), 20, "a")
        .unwrap()
        .unwrap();
    assert_eq!(b.created_at, a.created_at);
    assert!(change(&mut db, TODO, NOTE, false, Some(&a), 500, "a").is_err());
    assert_eq!(
        change(&mut db, TODO, NOTE, true, Some(&d), 600, "a").unwrap(),
        Some(b)
    );
    let mut altered = draft();
    altered.title = "another".into();
    assert!(create(&mut db, &altered, 700, "a").is_err());
    db.execute("UPDATE todos SET title='edited later'", [])
        .unwrap();
    create(&mut db, &draft(), 800, "a").unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM todos", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "edited later"
    );
}

#[test]
fn creation_failures_roll_back_entities_links_receipts_and_dirty_state() {
    for failure in [
        "CREATE TRIGGER fail BEFORE INSERT ON todos BEGIN SELECT RAISE(ABORT,'fail'); END",
        "CREATE TRIGGER fail BEFORE INSERT ON task_note_links BEGIN SELECT RAISE(ABORT,'fail'); END",
        "CREATE TRIGGER fail BEFORE UPDATE ON task_note_link_sync_state BEGIN SELECT RAISE(ABORT,'fail'); END",
        "CREATE TRIGGER fail BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'task_note_link_create.%' BEGIN SELECT RAISE(ABORT,'fail'); END",
    ] {
        let mut db = fixture();
        let before = scalar(&db, "SELECT todos_dirty_version FROM sync_runtime_state");
        db.execute_batch(failure).unwrap();
        assert!(create(&mut db, &draft(), 100, "a").is_err());
        assert_eq!(scalar(&db, "SELECT COUNT(*) FROM todos"), 0);
        assert_eq!(scalar(&db, "SELECT COUNT(*) FROM task_note_links"), 0);
        assert_eq!(scalar(&db, "SELECT COUNT(*) FROM app_metadata WHERE key LIKE 'task_note_link_create.%'"), 0);
        assert_eq!(scalar(&db, "SELECT todos_dirty_version FROM sync_runtime_state"), before);
        db.execute_batch("DROP TRIGGER fail").unwrap();
        create(&mut db, &draft(), 100, "a").unwrap();
    }
}

#[test]
fn limits_invalid_inputs_and_entity_lifecycle() {
    let mut db = fixture();
    let a = create(&mut db, &draft(), 100, "a").unwrap();
    for i in 2..=20 {
        let note = format!("123e4567-e89b-42d3-a456-{:012x}", i);
        db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'n','','default',0,1,1,'a')", [&note]).unwrap();
        change(&mut db, TODO, &note, true, None, 200, "a").unwrap();
    }
    let extra = "123e4567-e89b-42d3-a456-426614174099";
    db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'n','','default',0,1,1,'a')", [extra]).unwrap();
    assert_eq!(
        change(&mut db, TODO, extra, true, None, 200, "a").unwrap_err(),
        "TASK_NOTE_LINK_LIMIT"
    );
    change(&mut db, TODO, NOTE, false, Some(&a), 300, "a").unwrap();
    change(&mut db, TODO, extra, true, None, 400, "a").unwrap();
    db.execute("UPDATE todos SET archived_at=1", []).unwrap();
    assert!(change(&mut db, TODO, NOTE, true, None, 500, "a").is_err());
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) FROM task_note_links WHERE active=1"),
        20
    );
    db.execute("UPDATE todos SET archived_at=NULL,completed=1", [])
        .unwrap();
    crate::commands::clear_completed_todos_in_connection(&db).unwrap();
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) FROM task_note_links WHERE active=1"),
        0
    );
    assert_eq!(
        scalar(
            &db,
            "SELECT COUNT(*) FROM notes WHERE deleted_at IS NOT NULL"
        ),
        0
    );

    let mut db = fixture();
    let mut bad = draft();
    bad.due_date = Some("2026-02-30".into());
    assert!(create(&mut db, &bad, 100, "a").is_err());
    bad = draft();
    bad.group_uuid = Some("missing".into());
    assert!(create(&mut db, &bad, 100, "a").is_err());
    assert!(create(&mut db, &draft(), MAX_CLOCK, "a").is_err());
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM todos"), 0);
    let mut a = create(&mut db, &draft(), 100, "a").unwrap();
    a.updated_at = MAX_CLOCK;
    store::merge(
        &mut db,
        &LinkDocument {
            format_version: 1,
            links: vec![a.clone()],
        },
    )
    .unwrap();
    assert!(change(&mut db, TODO, NOTE, false, Some(&a), 100, "a").is_err());
}

#[test]
fn deleting_entities_rolls_back_on_link_failure_and_restore_does_not_relink() {
    for note in [false, true] {
        let mut db = fixture();
        create(&mut db, &draft(), 100, "a").unwrap();
        let id = scalar(&db, "SELECT id FROM todos");
        db.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON task_note_links BEGIN SELECT RAISE(ABORT,'fail'); END").unwrap();
        if note {
            assert!(crate::notes::soft_delete(&db, NOTE).is_err());
        } else {
            assert!(crate::commands::soft_delete_todo_in_connection(&mut db, id, None).is_err());
        }
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM notes WHERE deleted_at IS NOT NULL"
            ),
            0
        );
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM todos WHERE deleted_at IS NOT NULL"
            ),
            0
        );
        assert_eq!(store::snapshot(&db).unwrap().revision, 1);
        db.execute_batch("DROP TRIGGER fail").unwrap();
        if note {
            crate::notes::soft_delete(&db, NOTE).unwrap();
            crate::notes::restore(&db, NOTE).unwrap();
        } else {
            crate::commands::soft_delete_todo_in_connection(&mut db, id, None).unwrap();
            db.execute("UPDATE todos SET deleted_at=NULL WHERE id=?1", params![id])
                .unwrap();
        }
        assert_eq!(
            scalar(&db, "SELECT COUNT(*) FROM task_note_links WHERE active=1"),
            0
        );
    }
}
