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
fn request(c: &mut Connection, id: &str) -> WorkflowWrite {
    WorkflowWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: id.into(),
        state: "waiting".into(),
        reason: "  private reason  ".into(),
        review_date: Some(DAY.into()),
        date: DAY.into(),
        remove_from_plan: false,
        expected: list(c, DAY).unwrap().revision,
        expected_plan: None,
    }
}
fn waiting(c: &mut Connection, id: &str) {
    let r = request(c, id);
    write(c, &r, 10, "device").unwrap();
}
fn add_plan(c: &mut Connection, id: &str) {
    let r = plans::DailyPlanWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: id.into(),
        plan_date: DAY.into(),
        action: "add".into(),
        expected: plans::list(c, DAY).unwrap().revision,
    };
    plans::write(c, &r, 10, "device").unwrap();
}
#[test]
fn workflow_atomic_plan_removal_digest_retry_and_conflict() {
    let mut c = db();
    let id = task(&c);
    add_plan(&mut c, &id);
    let mut r = request(&mut c, &id);
    r.remove_from_plan = true;
    r.expected_plan = Some(plans::list(&mut c, DAY).unwrap().revision);
    let mut stale = r.clone();
    stale.expected_plan = Some("0".repeat(64));
    assert_eq!(
        write(&mut c, &stale, 10, "device").unwrap_err(),
        "WORKFLOW_CONFLICT"
    );
    assert!(snapshot(&mut c).unwrap().document.states.is_empty());
    c.execute_batch("CREATE TRIGGER fail_workflow_receipt BEFORE INSERT ON task_workflow_operations BEGIN SELECT RAISE(ABORT,'failure'); END").unwrap();
    assert_eq!(
        write(&mut c, &r, 10, "device").unwrap_err(),
        "WORKFLOW_DATABASE"
    );
    assert!(list(&mut c, DAY).unwrap().entries.is_empty());
    assert_eq!(plans::list(&mut c, DAY).unwrap().current.len(), 1);
    c.execute_batch("DROP TRIGGER fail_workflow_receipt")
        .unwrap();
    let applied = write(&mut c, &r, 10, "device").unwrap();
    assert!(plans::list(&mut c, DAY).unwrap().current.is_empty());
    assert!(!plans::snapshot(&mut c).unwrap().document.plans[0].included);
    assert_eq!(applied.entries[0].reason, r.reason);
    assert!(applied.entries[0].review_due);
    assert_eq!(write(&mut c, &r, 100, "other-device").unwrap(), applied);
    let receipt: String = c
        .query_row(
            "SELECT request_digest FROM task_workflow_operations",
            [],
            |q| q.get(0),
        )
        .unwrap();
    assert!(token(&receipt));
    assert!(!receipt.contains("private"));
    let mut changed = r.clone();
    changed.reason = "different".into();
    assert_eq!(
        write(&mut c, &changed, 10, "device").unwrap_err(),
        "WORKFLOW_CONFLICT"
    );
    changed = r.clone();
    changed.review_date = Some("2025-02-29".into());
    assert_eq!(
        write(&mut c, &changed, 10, "device").unwrap_err(),
        "WORKFLOW_INVALID"
    );
    c.execute(
        "UPDATE todos SET completed=1,updated_at=20 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert!(write(&mut c, &r, 100, "device").unwrap().entries.is_empty());
}
#[test]
fn workflow_lifecycle_and_authoritative_event_union_never_revive() {
    let mut c = db();
    let id = task(&c);
    waiting(&mut c, &id);
    c.execute(
        "UPDATE todos SET title='edit',updated_at=2 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    assert_eq!(list(&mut c, DAY).unwrap().entries.len(), 1);
    for (out, back) in [
        ("completed=1", "completed=0"),
        ("archived_at=3", "archived_at=NULL"),
        ("deleted_at=3", "deleted_at=NULL"),
    ] {
        waiting(&mut c, &id);
        let stale = snapshot(&mut c).unwrap().document;
        c.execute(
            &format!("UPDATE todos SET {out},updated_at=updated_at+1 WHERE uuid=?1"),
            [&id],
        )
        .unwrap();
        c.execute(
            &format!("UPDATE todos SET {back},updated_at=updated_at+1 WHERE uuid=?1"),
            [&id],
        )
        .unwrap();
        restore(&mut c, &stale).unwrap();
        assert!(list(&mut c, DAY).unwrap().entries.is_empty());
    }
    let mut remote = snapshot(&mut c).unwrap().document;
    remote.states[0].clock = 100;
    remote.states[0].basis = "100:61:0:-:-".into();
    remote.events.clear();
    restore(&mut c, &remote).unwrap();
    assert!(list(&mut c, DAY).unwrap().entries.is_empty());
    assert!(!snapshot(&mut c)
        .unwrap()
        .document
        .events
        .iter()
        .any(|e| e.event_id == "100:61:0:-:-"));
    let before = plans::snapshot(&mut c).unwrap().revision;
    remote.events.push(Event {
        task_uuid: id.clone(),
        event_id: "100:61:0:-:-".into(),
    });
    restore(&mut c, &remote).unwrap();
    assert!(plans::snapshot(&mut c).unwrap().revision > before);
    assert!(list(&mut c, DAY).unwrap().entries.is_empty());
}
#[test]
fn workflow_missing_parent_terminal_purge_and_late_documents() {
    let mut c = db();
    let id = task(&c);
    waiting(&mut c, &id);
    let stale = snapshot(&mut c).unwrap().document;
    c.execute("DELETE FROM todos WHERE uuid=?1", [&id]).unwrap();
    assert!(snapshot(&mut c).unwrap().document.states.is_empty());
    restore(&mut c, &stale).unwrap();
    assert!(list(&mut c, DAY).unwrap().entries.is_empty());
    let t = crate::purge::Terminal {
        kind: "todo".into(),
        uuid: id,
        operation_uuid: Uuid::new_v4().to_string(),
        purged_at: 50,
    };
    crate::purge::restore_terminals(&c, &[t]).unwrap();
    restore(&mut c, &stale).unwrap();
    assert!(snapshot(&mut c).unwrap().document.states.is_empty());
    let receipt: String = c
        .query_row(
            "SELECT request_digest FROM task_workflow_operations",
            [],
            |q| q.get(0),
        )
        .unwrap();
    assert!(token(&receipt));
}
#[test]
fn workflow_backup8_roundtrip_omitted_preserves_explicit_merges_and_validates() {
    let mut c = db();
    let id = task(&c);
    waiting(&mut c, &id);
    let raw = crate::data_exchange::planning_test_export(&mut c).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["format_version"], 8);
    assert_eq!(
        v["task_workflow"]["states"][0]["reason"],
        "  private reason  "
    );
    let mut target = db();
    crate::data_exchange::planning_test_import(&mut target, &raw).unwrap();
    assert_eq!(
        list(&mut target, DAY).unwrap().entries,
        list(&mut c, DAY).unwrap().entries
    );
    v.as_object_mut().unwrap().remove("task_workflow");
    crate::data_exchange::planning_test_import(&mut target, &v.to_string()).unwrap();
    assert_eq!(list(&mut target, DAY).unwrap().entries.len(), 1);
    v["format_version"] = 6.into();
    crate::data_exchange::planning_test_import(&mut target, &v.to_string()).unwrap();
    assert_eq!(list(&mut target, DAY).unwrap().entries.len(), 1);
    v["format_version"] = 8.into();
    v["task_workflow"] = serde_json::json!({"format_version":1,"events":[],"states":[]});
    crate::data_exchange::planning_test_import(&mut target, &v.to_string()).unwrap();
    assert_eq!(list(&mut target, DAY).unwrap().entries.len(), 1);
    v["task_workflow"]["extra"] = true.into();
    assert!(crate::data_exchange::planning_test_import(&mut target, &v.to_string()).is_err());
}
#[test]
fn workflow_upgrade26_atomic_retry_and_existing_events_are_dirty() {
    let mut c = db();
    let id = task(&c);
    c.execute(
        "UPDATE todos SET completed=1,updated_at=2 WHERE uuid=?1",
        [&id],
    )
    .unwrap();
    crate::db::remove_task_workflow_schema_for_test(&c);
    c.execute_batch("CREATE TRIGGER reject_workflow_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=26 BEGIN SELECT RAISE(ABORT,'failure'); END").unwrap();
    assert!(crate::db::migrate(&mut c).is_err());
    assert_eq!(
        c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='task_workflow_states'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    c.execute_batch("DROP TRIGGER reject_workflow_migration")
        .unwrap();
    crate::db::migrate(&mut c).unwrap();
    let s = snapshot(&mut c).unwrap();
    assert_eq!(s.document.events.len(), 1);
    assert!(s.revision > s.synced_revision);
    crate::db::migrate(&mut c).unwrap();
    assert_eq!(snapshot(&mut c).unwrap().revision, s.revision);
}
#[test]
fn workflow_ready_validation_ordering_and_target_invalidation() {
    let mut c = db();
    let a = task(&c);
    let b = task(&c);
    waiting(&mut c, &a);
    let mut r = request(&mut c, &b);
    r.review_date = None;
    write(&mut c, &r, 20, "device").unwrap();
    let s = list(&mut c, DAY).unwrap();
    assert_eq!(s.entries[0].task_uuid, a);
    assert_eq!(s.entries[1].task_uuid, b);
    let mut r = request(&mut c, &a);
    r.state = "ready".into();
    assert!(write(&mut c, &r, 30, "device").is_err());
    r.reason.clear();
    r.review_date = None;
    write(&mut c, &r, 30, "device").unwrap();
    assert_eq!(list(&mut c, DAY).unwrap().entries.len(), 1);
    let r = request(&mut c, &a);
    crate::sync_target::invalidate(&c).unwrap();
    crate::sync_target::activate(&c).unwrap();
    assert_eq!(
        write(&mut c, &r, 40, "device").unwrap_err(),
        "WORKFLOW_CONFLICT"
    );
    assert!(crate::sync_runtime_state::get_snapshot(&c)
        .unwrap()
        .dirty_domains
        .contains(&"workflow".into()));
}

#[test]
fn workflow_only_domain_blocks_fixed_legacy_migration() {
    let mut c = db();
    let id = task(&c);
    waiting(&mut c, &id);
    assert!(crate::daily_plan_protocol::is_empty(
        &plans::snapshot(&mut c).unwrap().document
    ));
    let s = crate::migration_preflight::read(&mut c).unwrap();
    assert!(s.blockers().contains(&"task_workflow_present".into()));
    assert_eq!(
        s.require_unchanged(&s).unwrap_err(),
        "MIGRATION_WORKFLOW_ACTIVE"
    );
}
