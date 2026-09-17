use crate::{archive as a, archive_batch as batch};
use rusqlite::Connection;
use serde::Deserialize;
#[derive(Deserialize)]
struct Fixtures {
    todo: String,
    second: String,
    device: String,
    operation: String,
    seed: Vec<String>,
    cases: Vec<Case>,
    fingerprint: String,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    action: a::Action,
    before: Vec<String>,
    after: Vec<String>,
    checks: Vec<Check>,
    error: Option<String>,
    now: Option<i64>,
}
#[derive(Deserialize)]
struct Check {
    sql: String,
    value: i64,
}
fn fixtures() -> Fixtures {
    serde_json::from_str(include_str!("../../docs/fixtures/archive-storage-v1.json")).unwrap()
}
fn seed() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    for sql in fixtures().seed {
        c.execute_batch(&sql).unwrap();
    }
    c
}
#[test]
fn archive_shared_storage_cases() {
    let f = fixtures();
    for case in f.cases {
        let mut c = seed();
        for sql in case.before {
            c.execute_batch(&sql).unwrap();
        }
        let p = a::preview(&mut c, &f.todo).unwrap();
        for sql in case.after {
            c.execute_batch(&sql).unwrap();
        }
        let outcome = a::apply(
            &mut c,
            &f.operation,
            case.action,
            &p.expected,
            case.now.unwrap_or(200),
            &f.device,
        );
        match case.error.as_deref() {
            None => {
                outcome.unwrap_or_else(|e| panic!("{}: {e}", case.id));
            }
            Some("DATABASE") => assert_eq!(
                outcome.unwrap_err(),
                "ARCHIVE_DATABASE_FAILED",
                "{}",
                case.id
            ),
            Some(code) => assert_eq!(outcome.unwrap_err(), code, "{}", case.id),
        }
        for check in case.checks {
            assert_eq!(
                c.query_row(&check.sql, [], |r| r.get::<_, i64>(0)).unwrap(),
                check.value,
                "{}",
                case.id
            );
        }
        if case.error.is_some() {
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM app_metadata WHERE key LIKE 'archive.op.v1:%'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        } else {
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM task_checklist_items WHERE active=1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
}
#[test]
fn archive_pagination_literals_snapshot_and_retries() {
    let f = fixtures();
    let mut c = seed();
    let page = a::list(&mut c, "", 1, None).unwrap();
    assert_eq!(page.items[0].expected.uuid, f.todo);
    assert_eq!(
        a::list(&mut c, "", 1, page.next.as_ref()).unwrap().items[0]
            .expected
            .uuid,
        f.second
    );
    assert_eq!(a::list(&mut c, "%_", 50, None).unwrap().items.len(), 1);
    assert!(a::list(&mut c, "' OR 1=1", 50, None)
        .unwrap()
        .items
        .is_empty());
    assert!(a::list(&mut c, "other", 50, page.next.as_ref()).is_err());
    assert!(a::list(&mut c, "", 0, None).is_err());
    assert!(a::preview(&mut c, "invalid").is_err());
    let p = &page.items[0];
    assert_eq!(p.expected.fingerprint, f.fingerprint);
    let before_items = p.checklist_json.clone();
    let before_links = p.links_json.clone();
    let result = a::apply(
        &mut c,
        &f.operation,
        a::Action::Reopen,
        &p.expected,
        200,
        &f.device,
    )
    .unwrap();
    assert_eq!(result.result_version, 200);
    c.execute(
        "UPDATE todos SET title='Later user edit' WHERE uuid=?1",
        [&f.todo],
    )
    .unwrap();
    assert_eq!(
        a::apply(
            &mut c,
            &f.operation,
            a::Action::Reopen,
            &p.expected,
            300,
            &f.device
        )
        .unwrap()
        .outcome,
        "already_applied"
    );
    assert_eq!(
        a::apply(
            &mut c,
            &f.operation,
            a::Action::Delete,
            &p.expected,
            300,
            &f.device
        )
        .unwrap_err(),
        "ARCHIVE_OPERATION_CONFLICT"
    );
    assert_eq!(
        c.query_row("SELECT title FROM todos WHERE uuid=?1", [&f.todo], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "Later user edit"
    );
    c.execute("UPDATE todos SET archived_at=300 WHERE uuid=?1", [&f.todo])
        .unwrap();
    let later = a::preview(&mut c, &f.todo).unwrap();
    assert_eq!(later.checklist_json, before_items);
    assert_eq!(later.links_json, before_links);
    let raw: String = c
        .query_row(
            "SELECT value FROM app_metadata WHERE key LIKE 'archive.op.v1:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!raw.contains("Keep body") && !raw.contains("Archived report"));
    let tx = c.transaction().unwrap();
    a::invalidate_in_transaction(&tx).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        a::apply(
            &mut c,
            &f.operation,
            a::Action::Delete,
            &later.expected,
            400,
            &f.device
        )
        .unwrap_err(),
        "ARCHIVE_SCOPE_CHANGED"
    );
}
#[test]
fn archive_batch_fixed_targets_partial_conflicts_and_rollback() {
    let f = fixtures();
    let mut c = seed();
    let targets: Vec<_> = a::select_all(&mut c, "")
        .unwrap()
        .into_iter()
        .map(|p| p.expected)
        .collect();
    batch::prepare(&mut c, &f.operation, a::Action::Unarchive, &targets).unwrap();
    assert_eq!(batch::pending(&mut c).unwrap().len(), 1);
    assert!(batch::prepare(&mut c, &f.operation, a::Action::Delete, &targets).is_err());
    c.execute_batch("CREATE TRIGGER fail_batch BEFORE UPDATE ON todos WHEN OLD.title='Archived second' BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(batch::run(&mut c, &f.operation, 50, 200, &f.device).is_err());
    assert!(batch::get(&c, &f.operation).unwrap().results.is_empty());
    assert_eq!(a::select_all(&mut c, "").unwrap().len(), 2);
    c.execute_batch("DROP TRIGGER fail_batch").unwrap();
    let partial = batch::run(&mut c, &f.operation, 1, 200, &f.device).unwrap();
    assert_eq!(partial.results.len(), 1);
    c.execute(
        "UPDATE todos SET title='changed while paused' WHERE uuid=?1",
        [&f.second],
    )
    .unwrap();
    c.execute("INSERT INTO todos(uuid,title,completed,sort_order,created_at,updated_at,archived_at,updated_by) VALUES(?1,'new archive',1,3,1,100,100,?2)",rusqlite::params![Uuid::new_v4().to_string(),f.device]).unwrap();
    let end = batch::run(&mut c, &f.operation, 50, 300, &f.device).unwrap();
    assert_eq!(end.results.len(), 2);
    assert_eq!(end.results[1].error.as_deref(), Some("ARCHIVE_CONFLICT"));
    assert_eq!(
        batch::run(&mut c, &f.operation, 50, 400, &f.device).unwrap(),
        end
    );
    assert_eq!(a::select_all(&mut c, "").unwrap().len(), 2);
}
use uuid::Uuid;
#[test]
fn archive_batch_dismiss_retains_receipts_and_scope_isolates_progress() {
    let f = fixtures();
    let mut c = seed();
    let targets: Vec<_> = a::select_all(&mut c, "")
        .unwrap()
        .into_iter()
        .map(|p| p.expected)
        .collect();
    batch::prepare(&mut c, &f.operation, a::Action::Delete, &targets).unwrap();
    let finished = batch::run(&mut c, &f.operation, 50, 200, &f.device).unwrap();
    assert_eq!(batch::pending(&mut c).unwrap(), vec![finished.clone()]);
    batch::dismiss(&mut c, &f.operation).unwrap();
    batch::dismiss(&mut c, &f.operation).unwrap();
    assert!(batch::pending(&mut c).unwrap().is_empty());
    assert_eq!(
        batch::run(&mut c, &f.operation, 50, 300, &f.device).unwrap(),
        finished
    );
    let mut other = seed();
    batch::prepare(&mut other, &f.operation, a::Action::Unarchive, &targets).unwrap();
    other
        .execute("DELETE FROM app_metadata WHERE key='archive.scope.v1'", [])
        .unwrap();
    assert!(batch::pending(&mut other).unwrap().is_empty());
    assert!(batch::run(&mut other, &f.operation, 50, 300, &f.device).is_err());
}
#[test]
fn archive_receipts_and_batches_survive_reopen() {
    let f = fixtures();
    let path = std::env::temp_dir().join(format!("eggdone-archive-{}.sqlite", Uuid::new_v4()));
    {
        let mut c = Connection::open(&path).unwrap();
        crate::db::migrate(&mut c).unwrap();
        for sql in &f.seed {
            c.execute_batch(sql).unwrap();
        }
        let selection: Vec<_> = a::select_all(&mut c, "")
            .unwrap()
            .into_iter()
            .map(|p| p.expected)
            .collect();
        batch::prepare(&mut c, &f.operation, a::Action::Unarchive, &selection).unwrap();
        batch::run(&mut c, &f.operation, 1, 200, &f.device).unwrap();
    }
    {
        let mut c = Connection::open(&path).unwrap();
        crate::db::migrate(&mut c).unwrap();
        assert_eq!(batch::get(&c, &f.operation).unwrap().results.len(), 1);
        assert_eq!(batch::pending(&mut c).unwrap()[0].results.len(), 1);
        let job = batch::run(&mut c, &f.operation, 50, 201, &f.device).unwrap();
        assert_eq!(job.results.len(), 2);
        assert!(job.results.iter().all(|r| r.error.is_none()));
        assert!(a::select_all(&mut c, "").unwrap().is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn archive_ended_batch_cannot_process_remaining_targets() {
    let f = fixtures();
    let mut c = seed();
    let targets: Vec<_> = a::select_all(&mut c, "")
        .unwrap()
        .into_iter()
        .map(|p| p.expected)
        .collect();
    batch::prepare(&mut c, &f.operation, a::Action::Delete, &targets).unwrap();
    batch::run(&mut c, &f.operation, 1, 200, &f.device).unwrap();
    batch::dismiss(&mut c, &f.operation).unwrap();
    assert_eq!(
        batch::run(&mut c, &f.operation, 50, 300, &f.device).unwrap_err(),
        "ARCHIVE_BATCH_ENDED"
    );
    assert_eq!(batch::get(&c, &f.operation).unwrap().results.len(), 1);
    assert_eq!(a::select_all(&mut c, "").unwrap().len(), 1);
}

#[test]
fn archive_works_after_v20_upgrade() {
    let f = fixtures();
    let mut c = seed();
    // Reconstruct the previous schema by removing only migration 21 additions.
    c.execute_batch("DROP TABLE task_templates; DROP TABLE task_template_sync_state; DROP TABLE task_template_operations; DELETE FROM schema_migrations WHERE version=21;").unwrap();
    crate::db::migrate(&mut c).unwrap();
    let p = a::preview(&mut c, &f.todo).unwrap();
    assert_eq!(p.expected.fingerprint, f.fingerprint);
    a::apply(
        &mut c,
        &f.operation,
        a::Action::Reopen,
        &p.expected,
        200,
        &f.device,
    )
    .unwrap();
}

#[test]
#[ignore = "Requires isolated Harmony JSON exchange paths from test-archive-storage.cjs"]
fn archive_cross_client_exchange() {
    let input = std::env::var("EGGDONE_ARCHIVE_INPUT").unwrap();
    let output = std::env::var("EGGDONE_ARCHIVE_OUTPUT").unwrap();
    let doc: crate::sync::SyncDocument =
        serde_json::from_str(&std::fs::read_to_string(input).unwrap()).unwrap();
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    crate::sync::merge_remote_document(&mut c, &doc, 300).unwrap();
    let f = fixtures();
    let before = crate::sync::build_document(&c, 300).unwrap();
    let todo = before.todos.iter().find(|t| t.uuid == f.todo).unwrap();
    assert!(todo.completed);
    assert_eq!(todo.archived_at, None);
    assert_eq!(todo.reminder_at, None);
    c.execute(
        "UPDATE todos SET archived_at=300,updated_at=300 WHERE uuid=?1",
        [&f.todo],
    )
    .unwrap();
    let p = a::preview(&mut c, &f.todo).unwrap();
    a::apply(
        &mut c,
        &f.operation,
        a::Action::Reopen,
        &p.expected,
        400,
        &f.device,
    )
    .unwrap();
    let after = crate::sync::build_document(&c, 400).unwrap();
    let result = after.todos.iter().find(|t| t.uuid == f.todo).unwrap();
    assert!(!result.completed);
    assert_eq!(result.due_date, todo.due_date);
    assert_eq!(result.repeat_rule, None);
    std::fs::write(output, serde_json::to_vec(&after).unwrap()).unwrap();
}
