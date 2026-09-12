use crate::trash::{self, TrashKind};
use rusqlite::Connection;
use serde::Deserialize;

#[derive(Deserialize)]
struct Cases {
    seed: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    kind: TrashKind,
    before: Vec<String>,
    after: Vec<String>,
    error: Option<String>,
    checks: Vec<Check>,
}
#[derive(Deserialize)]
struct Check {
    sql: String,
    value: i64,
}
const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";
fn fixtures() -> Cases {
    serde_json::from_str(include_str!("../../docs/fixtures/trash-restore-v1.json")).unwrap()
}
fn seed() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    for sql in fixtures().seed {
        db.execute_batch(&sql).unwrap();
    }
    crate::db::migrate(&mut db).unwrap();
    db
}
#[test]
fn trash_shared_restore_contract() {
    for case in fixtures().cases {
        let mut db = seed();
        for sql in case.before {
            db.execute_batch(&sql).unwrap();
        }
        let uuid = if case.kind == TrashKind::Todo {
            TODO
        } else {
            NOTE
        };
        let expected = trash::preview(&db, case.kind, uuid).unwrap();
        for sql in case.after {
            db.execute_batch(&sql).unwrap();
        }
        let outcome = trash::restore(&mut db, &expected, 50, "local");
        match case.error.as_deref() {
            None => assert!(outcome.is_ok(), "{}: {:?}", case.id, outcome),
            Some("DATABASE") => {
                assert_eq!(outcome.unwrap_err(), "TRASH_DATABASE_FAILED", "{}", case.id)
            }
            Some(code) => assert_eq!(outcome.unwrap_err(), code, "{}", case.id),
        }
        for check in case.checks {
            assert_eq!(
                db.query_row(&check.sql, [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                check.value,
                "{}",
                case.id
            );
        }
    }
}
#[test]
fn trash_pagination_and_link_tombstones() {
    let mut db = seed();
    assert_eq!(trash::list(&db, 0, 1).unwrap()[0].kind, TrashKind::Note);
    assert_eq!(trash::list(&db, 1, 1).unwrap()[0].kind, TrashKind::Todo);
    assert!(trash::list(&db, 2, 1).unwrap().is_empty());
    assert!(trash::list(&db, 0, 101).is_err());
    assert!(trash::preview(&db, TrashKind::Todo, "' OR 1=1").is_err());
    db.execute_batch("UPDATE todos SET deleted_at=NULL; UPDATE notes SET deleted_at=NULL;")
        .unwrap();
    crate::task_note_link_operations::change(&mut db, TODO, NOTE, true, None, 10, "local").unwrap();
    db.execute_batch("UPDATE todos SET deleted_at=100;")
        .unwrap();
    let p = trash::preview(&db, TrashKind::Todo, TODO).unwrap();
    db.execute_batch("CREATE TRIGGER reject_restore BEFORE UPDATE ON todos WHEN NEW.deleted_at IS NULL BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(trash::restore(&mut db, &p, 200, "local").is_err());
    assert_eq!(
        db.query_row("SELECT active FROM task_note_links", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER reject_restore").unwrap();
    trash::restore(&mut db, &p, 200, "local").unwrap();
    assert_eq!(
        db.query_row("SELECT active FROM task_note_links", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn trash_survives_database_reopen() {
    let path = std::env::temp_dir().join(format!("eggdone-trash-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        for sql in fixtures().seed {
            db.execute_batch(&sql).unwrap();
        }
    }
    {
        let mut db = Connection::open(&path).unwrap();
        crate::db::migrate(&mut db).unwrap();
        assert_eq!(trash::list(&db, 0, 50).unwrap().len(), 2);
        let preview = trash::preview(&db, TrashKind::Note, NOTE).unwrap();
        trash::restore(&mut db, &preview, 200, "local").unwrap();
    }
    {
        let db = Connection::open(&path).unwrap();
        assert_eq!(trash::list(&db, 0, 50).unwrap().len(), 1);
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    std::fs::remove_file(path).unwrap();
}
