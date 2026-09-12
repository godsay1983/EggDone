use crate::note_history;
use rusqlite::{params, Connection};
use serde::Deserialize;

const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
#[derive(Deserialize)]
struct Cases {
    seed: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    before: Vec<String>,
    after: Vec<String>,
    error: Option<String>,
    changed: bool,
    checks: Vec<Check>,
}
#[derive(Deserialize)]
struct Check {
    sql: String,
    value: i64,
}
fn cases() -> Cases {
    serde_json::from_str(include_str!("../../docs/fixtures/note-history-v1.json")).unwrap()
}
fn seed() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    for sql in cases().seed {
        db.execute_batch(&sql).unwrap();
    }
    db
}
fn number(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn note_history_shared_restore_contract() {
    for case in cases().cases {
        let mut db = seed();
        for sql in case.before {
            db.execute_batch(&sql).unwrap();
        }
        let expected = note_history::preview(&db, NOTE, 1).unwrap();
        for sql in case.after {
            db.execute_batch(&sql).unwrap();
        }
        let result = note_history::restore(&mut db, &expected, 50, "local");
        match case.error {
            Some(error) => assert_eq!(result.unwrap_err(), error, "{}", case.id),
            None => assert_eq!(result.unwrap(), case.changed, "{}", case.id),
        }
        for check in case.checks {
            assert_eq!(number(&db, &check.sql), check.value, "{}", case.id);
        }
    }
}

#[test]
fn note_history_capture_lifecycle_and_retention() {
    let mut db = seed();
    db.execute_batch("UPDATE notes SET title=title,content=content; UPDATE notes SET pinned=1,color='blue'; UPDATE notes SET deleted_at=40;").unwrap();
    assert_eq!(number(&db, "SELECT count(*) FROM note_history"), 1);
    assert_eq!(
        note_history::list(&db, NOTE).unwrap_err(),
        "NOTE_HISTORY_NOTE_UNAVAILABLE"
    );
    db.execute_batch("UPDATE notes SET deleted_at=NULL")
        .unwrap();
    let p = note_history::preview(&db, NOTE, 1).unwrap();
    let mut forged = p.clone();
    forged.entry.text.content = "Forged".into();
    assert_eq!(
        note_history::restore(&mut db, &forged, 50, "local").unwrap_err(),
        "NOTE_HISTORY_CONFLICT"
    );
    assert!(note_history::preview(&db, "' OR 1=1", 1).is_err());
    assert!(note_history::preview(&db, NOTE, 0).is_err());
    for i in 0..140 {
        db.execute("UPDATE notes SET content=?", [format!("Saved {i}")])
            .unwrap();
    }
    let summaries = note_history::list(&db, NOTE).unwrap();
    assert_eq!(summaries.len(), 100);
    assert!(summaries.windows(2).all(|x| x[0].id > x[1].id));
    assert_eq!(
        note_history::restore(&mut db, &p, 50, "local").unwrap_err(),
        "NOTE_HISTORY_NOT_FOUND"
    );
    for i in 0..1100 {
        db.execute("INSERT INTO note_history(note_uuid,title,content,updated_at,updated_by,captured_at) VALUES(?,'x','body',1,'by',1)", [format!("fixture-{i}")]).unwrap();
    }
    assert_eq!(number(&db, "SELECT count(*) FROM note_history"), 1000);
    db.execute("UPDATE notes SET content='Last text'", [])
        .unwrap();
    assert_eq!(note_history::list(&db, NOTE).unwrap().len(), 1);
    db.execute("DELETE FROM notes WHERE uuid=?", [NOTE])
        .unwrap();
    assert_eq!(number(&db, "SELECT count(*) FROM note_history WHERE note_uuid='123e4567-e89b-42d3-a456-426614174001'"), 0);
}

#[test]
fn note_history_sync_capture_and_wire_exclusion() {
    let mut db = seed();
    let by = "123e4567-e89b-42d3-a456-426614174099";
    db.execute("UPDATE notes SET updated_by=?", [by]).unwrap();
    db.execute(
        "UPDATE app_metadata SET value=? WHERE key='device_id'",
        [by],
    )
    .unwrap();
    let mut remote = crate::note_sync::build_document(&db, 100).unwrap();
    remote.notes[0].content = "Synchronized replacement".into();
    remote.notes[0].updated_at = 100;
    crate::note_sync::merge_remote_document(&mut db, &remote, 101).unwrap();
    assert_eq!(
        number(
            &db,
            "SELECT count(*) FROM note_history WHERE content='Current body'"
        ),
        1
    );
    crate::note_sync::merge_remote_document(&mut db, &remote, 102).unwrap();
    assert_eq!(number(&db, "SELECT count(*) FROM note_history"), 2);
    let wire = serde_json::to_value(crate::note_sync::build_document(&db, 102).unwrap()).unwrap();
    assert!(wire.get("note_history").is_none());
    assert!(!wire.to_string().contains("Original body"));
    let p = note_history::preview(&db, NOTE, 2).unwrap();
    note_history::restore(&mut db, &p, 103, by).unwrap();
    let restored = crate::note_sync::build_document(&db, 104).unwrap();
    assert_eq!(restored.notes[0].content, "Current body");
    assert_eq!(restored.notes[0].updated_at, 103);
}

#[test]
fn note_history_upgrade_rollback_and_reopen() {
    let path =
        std::env::temp_dir().join(format!("eggdone-history-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        for sql in cases().seed {
            db.execute_batch(&sql).unwrap();
        }
        db.execute_batch("DROP TRIGGER notes_capture_history; DROP TRIGGER notes_delete_history; DROP TABLE note_history; DELETE FROM schema_migrations WHERE version=19;
          CREATE TRIGGER fail_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=19 BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
        assert!(crate::db::migrate(&mut db).is_err());
        assert_eq!(
            number(&db, "SELECT MAX(version) FROM schema_migrations"),
            18
        );
        assert_eq!(
            number(
                &db,
                "SELECT count(*) FROM sqlite_master WHERE name='note_history'"
            ),
            0
        );
        assert_eq!(
            number(
                &db,
                "SELECT count(*) FROM notes WHERE content='Current body'"
            ),
            1
        );
        db.execute_batch("DROP TRIGGER fail_migration").unwrap();
        crate::db::migrate(&mut db).unwrap();
        assert!(note_history::list(&db, NOTE).unwrap().is_empty());
        db.execute("UPDATE notes SET content=?", params!["After upgrade"])
            .unwrap();
    }
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        let entries = note_history::list(&db, NOTE).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].excerpt, "Current body");
        let p = note_history::preview(&db, NOTE, entries[0].id).unwrap();
        note_history::restore(&mut db, &p, 50, "local").unwrap();
    }
    std::fs::remove_file(path).unwrap();
}
