use super::*;
use uuid::Uuid;
const DAY: &str = "2026-09-19";
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
fn task(c: &Connection) -> String {
    let id = Uuid::new_v4().to_string();
    c.execute("INSERT INTO todos(uuid,title,completed,created_at,updated_at,updated_by,sort_order) VALUES(?1,'task',0,1,1,'device',7)",[&id]).unwrap();
    id
}
fn action(c: &mut Connection, id: &str, action: &str, date: &str) -> DailyPlanWrite {
    DailyPlanWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: id.into(),
        plan_date: date.into(),
        action: action.into(),
        expected: list(c, date).unwrap().revision,
    }
}
fn apply(c: &mut Connection, id: &str, a: &str) {
    let r = action(c, id, a, DAY);
    write(c, &r, 100, "device").unwrap();
}
#[test]
fn lifecycle_invalidates_without_changing_task_fields() {
    let mut c = db();
    let id = task(&c);
    apply(&mut c, &id, "add");
    c.execute(
        "UPDATE todos SET title='edited',updated_at=2 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert_eq!(list(&mut c, DAY).unwrap().current.len(), 1);
    c.execute(
        "UPDATE todos SET completed=1,updated_at=3 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert_eq!(list(&mut c, DAY).unwrap().current[0].status, "completed");
    c.execute(
        "UPDATE todos SET completed=0,updated_at=4 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert!(list(&mut c, DAY).unwrap().current.is_empty());
    apply(&mut c, &id, "add");
    c.execute(
        "UPDATE todos SET deleted_at=5,updated_at=5 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    c.execute(
        "UPDATE todos SET deleted_at=NULL,updated_at=6 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert!(list(&mut c, DAY).unwrap().current.is_empty());
    let order: i64 = c
        .query_row("SELECT sort_order FROM todos WHERE uuid=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(order, 7);
}
#[test]
fn reorder_is_transactional_idempotent_and_detects_stale_ui() {
    let mut c = db();
    let a = task(&c);
    let b = task(&c);
    apply(&mut c, &a, "add");
    apply(&mut c, &b, "add");
    let stale = action(&mut c, &a, "remove", DAY);
    let r = action(&mut c, &b, "up", DAY);
    let first = write(&mut c, &r, 100, "device").unwrap();
    assert_eq!(first.current[0].task_uuid, b);
    assert_eq!(write(&mut c, &r, 200, "device").unwrap(), first);
    assert_eq!(
        write(&mut c, &stale, 100, "device").unwrap_err(),
        "PLAN_CONFLICT"
    );
}
#[test]
fn yesterday_suggestions_are_explicit_and_date_validation_is_real() {
    let mut c = db();
    let a = task(&c);
    let r = action(&mut c, &a, "add", "2026-09-18");
    write(&mut c, &r, 100, "device").unwrap();
    let s = list(&mut c, DAY).unwrap();
    assert!(s.current.is_empty());
    assert_eq!(s.previous.len(), 1);
    apply(&mut c, &a, "add");
    assert!(list(&mut c, DAY).unwrap().previous.is_empty());
    assert!(list(&mut c, "2026-02-30").is_err());
}
#[test]
fn remote_old_plan_does_not_revive_and_missing_parent_never_shows() {
    let mut c = db();
    let a = task(&c);
    apply(&mut c, &a, "add");
    let old = snapshot(&mut c).unwrap().document;
    c.execute(
        "UPDATE todos SET archived_at=2,updated_at=2 WHERE uuid=?1",
        [&a],
    )
    .unwrap();
    c.execute(
        "UPDATE todos SET archived_at=NULL,updated_at=3 WHERE uuid=?1",
        [&a],
    )
    .unwrap();
    restore(&mut c, &old).unwrap();
    assert!(list(&mut c, DAY).unwrap().current.is_empty());
    c.execute("DELETE FROM todos WHERE uuid=?1", [&a]).unwrap();
    assert!(snapshot(&mut c).unwrap().document.plans.is_empty());
    restore(&mut c, &old).unwrap();
    assert!(list(&mut c, DAY).unwrap().current.is_empty());
}
#[test]
fn migration_is_repeatable_and_upgrade_does_not_change_tasks() {
    let mut c = db();
    let a = task(&c);
    apply(&mut c, &a, "add");
    let before = list(&mut c, DAY).unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(list(&mut c, DAY).unwrap(), before);
}

#[test]
fn v23_upgrade_failure_is_atomic_and_retry_preserves_tasks() {
    let mut c = db();
    crate::db::remove_daily_plan_schema_for_test(&c);
    let a = task(&c);
    c.execute_batch("CREATE TRIGGER reject_plan_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=24 BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(crate::db::migrate(&mut c).is_err());
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='daily_plans'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER reject_plan_migration")
        .unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(
        c.query_row("SELECT title FROM todos WHERE uuid=?1", [&a], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "task"
    );
    apply(&mut c, &a, "add");
    assert_eq!(list(&mut c, DAY).unwrap().current.len(), 1);
}

#[test]
fn v24_trigger_repair_preserves_existing_planning() {
    let mut c = db();
    let a = task(&c);
    apply(&mut c, &a, "add");
    let before = list(&mut c, DAY).unwrap();
    let early=include_str!("migrations/025_daily_plan_remote_guard.sql").replace(" AND NOT EXISTS(SELECT 1 FROM app_metadata WHERE key='daily.plan.remote-apply.v1' AND value='1')\n","");
    c.execute_batch(&early).unwrap();
    c.execute("DELETE FROM schema_migrations WHERE version=25", [])
        .unwrap();
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(list(&mut c, DAY).unwrap(), before);
    let sql: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='daily_plan_task_lifecycle'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("daily.plan.remote-apply.v1"));
}
