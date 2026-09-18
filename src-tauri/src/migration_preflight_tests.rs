use crate::{db, migration_preflight as p, sync_runtime_state as runtime};
use rusqlite::Connection;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    baseline_sql: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    sql: String,
    blockers: Vec<String>,
    invalid: bool,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/migration-local-preflight-v1.json"
    ))
    .unwrap()
}
fn database() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    db::migrate(&mut db).unwrap();
    db.execute_batch(&fixture().baseline_sql).unwrap();
    db
}

#[test]
fn shared_local_preflight_cases_and_stale_previews() {
    for case in fixture().cases {
        let mut db = database();
        let before = p::read(&mut db).unwrap();
        db.execute_batch(&case.sql).unwrap();
        let changes = db.total_changes();
        let result = p::read(&mut db);
        assert_eq!(db.total_changes(), changes, "{} must be read-only", case.id);
        if case.invalid {
            assert_eq!(
                result.unwrap_err(),
                "MIGRATION_LOCAL_STATE_INVALID",
                "{}",
                case.id
            );
        } else {
            let current = result.unwrap();
            let mut actual = current.blockers();
            let mut expected = case.blockers;
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "{}", case.id);
            if case.sql.is_empty() {
                before.require_unchanged(&current).unwrap();
            } else {
                assert!(before.require_unchanged(&current).is_err(), "{}", case.id);
            }
        }
    }
}

#[test]
fn dirty_revision_and_real_ack_never_accept_a_stale_preview() {
    let mut db = database();
    let before = p::read(&mut db).unwrap();
    db.execute_batch("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES('t','fixture',0,1,1,'fixture')").unwrap();
    let attempt = runtime::begin_attempt(&db).unwrap();
    db.execute_batch("UPDATE todos SET title='newer'").unwrap();
    assert!(!runtime::mark_domain_synced(
        &db,
        runtime::SyncDomain::Todos,
        attempt.for_domain(runtime::SyncDomain::Todos)
    )
    .unwrap());
    assert!(p::read(&mut db)
        .unwrap()
        .blockers()
        .contains(&"pending_todos".into()));
    let revision = runtime::domain_revision(&db, runtime::SyncDomain::Todos).unwrap();
    assert!(runtime::mark_domain_synced(&db, runtime::SyncDomain::Todos, revision).unwrap());
    runtime::record_success(&db).unwrap();
    let now = p::read(&mut db).unwrap();
    assert!(now.blockers().is_empty());
    assert_eq!(
        before.require_unchanged(&now).unwrap_err(),
        "MIGRATION_LOCAL_CHANGED"
    );
}

#[test]
fn reopens_existing_database_without_resetting_pending_work() {
    let file =
        std::env::temp_dir().join(format!("eggdone-preflight-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let mut db = Connection::open(&file).unwrap();
        db::migrate(&mut db).unwrap();
        db.execute_batch(&fixture().baseline_sql).unwrap();
        db.execute_batch("INSERT INTO notes(uuid,title,created_at,updated_at,updated_by) VALUES('n','fixture',1,1,'fixture')").unwrap();
    }
    let mut db = Connection::open(&file).unwrap();
    db::migrate(&mut db).unwrap();
    assert_eq!(p::read(&mut db).unwrap().blockers(), vec!["pending_notes"]);
    drop(db);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn inspection_obeys_sync_exclusion_and_releases_it() {
    let db = db::Database {
        connection: std::sync::Mutex::new(database()),
    };
    let runtime = crate::s3_sync::SyncRuntime::default();
    let guard = runtime.acquire().unwrap();
    assert!(p::inspect(&db, &runtime).is_err());
    drop(guard);
    let before = p::inspect(&db, &runtime).unwrap();
    before
        .require_unchanged(&p::inspect(&db, &runtime).unwrap())
        .unwrap();
    db.connection
        .lock()
        .unwrap()
        .execute_batch("DELETE FROM sync_runtime_state")
        .unwrap();
    assert!(p::inspect(&db, &runtime).is_err());
    assert!(runtime.acquire().is_ok());
}
