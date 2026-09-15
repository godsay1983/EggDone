use crate::task_template_protocol::{self as p, TaskTemplate, TemplatesDocument};
use crate::task_template_store::{self as s, TemplateWrite};
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../docs/fixtures/task-templates-v1.json")).unwrap()
}
fn base() -> TaskTemplate {
    serde_json::from_value(fixtures()["base"].clone()).unwrap()
}
fn doc(row: TaskTemplate) -> TemplatesDocument {
    TemplatesDocument {
        format_version: 1,
        templates: vec![row],
    }
}
fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
fn request(expected: Option<TaskTemplate>) -> TemplateWrite {
    TemplateWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        uuid: expected
            .as_ref()
            .map_or_else(|| Uuid::new_v4().to_string(), |r| r.uuid.clone()),
        content: expected.as_ref().unwrap_or(&base()).content.clone(),
        expected,
        deleted: false,
    }
}
#[test]
fn template_protocol_shared_fixtures_and_merge_laws() {
    let f = fixtures();
    let canonical: Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-templates-v1-canonical.json"
    ))
    .unwrap();
    for c in f["valid"].as_array().unwrap() {
        let parsed = p::parse(&c["document"].to_string()).unwrap();
        assert_eq!(
            p::encode(&parsed).unwrap(),
            canonical[c["id"].as_str().unwrap()].as_str().unwrap()
        );
        assert_eq!(
            p::parse(&p::encode(&parsed).unwrap()).unwrap(),
            parsed,
            "{}",
            c["id"]
        );
    }
    for c in f["invalid"].as_array().unwrap() {
        assert!(p::parse(&c["document"].to_string()).is_err(), "{}", c["id"]);
    }
    assert!(p::parse(&" ".repeat(4194305)).is_err());
    for c in f["merges"].as_array().unwrap() {
        let a = p::parse(&c["left"].to_string()).unwrap();
        let b = p::parse(&c["right"].to_string()).unwrap();
        if c["error"] == true {
            assert!(p::merge(&a, &b).is_err());
            assert!(p::merge(&b, &a).is_err());
        } else {
            let expected = p::parse(&c["expected"].to_string()).unwrap();
            assert_eq!(p::merge(&a, &b).unwrap(), expected);
            assert_eq!(p::merge(&b, &a).unwrap(), expected);
        }
    }
    let variants: Vec<TaskTemplate> = serde_json::from_value(f["variants"].clone()).unwrap();
    for a in &variants {
        for b in &variants {
            for c in &variants {
                let a = doc(a.clone());
                let b = doc(b.clone());
                let c = doc(c.clone());
                assert_eq!(p::merge(&a, &a).unwrap(), a);
                assert_eq!(
                    p::merge(&p::merge(&a, &b).unwrap(), &c).unwrap(),
                    p::merge(&a, &p::merge(&b, &c).unwrap()).unwrap()
                );
            }
        }
    }
}
#[test]
fn template_storage_atomic_retry_and_source_independence() {
    let mut db = db();
    let r = request(None);
    db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'original',0,1,1,'device')",[Uuid::new_v4().to_string()]).unwrap();
    let before = s::snapshot(&mut db).unwrap();
    db.execute_batch("CREATE TRIGGER fail_template_receipt BEFORE INSERT ON task_template_operations BEGIN SELECT RAISE(ABORT,'receipt failure'); END;").unwrap();
    assert!(s::save(&mut db, &r, 10, "device")
        .unwrap_err()
        .contains("receipt failure"));
    assert_eq!(s::snapshot(&mut db).unwrap(), before);
    db.execute_batch("DROP TRIGGER fail_template_receipt")
        .unwrap();
    let saved = s::save(&mut db, &r, 10, "device").unwrap();
    let after = s::snapshot(&mut db).unwrap();
    assert_eq!(s::save(&mut db, &r, 99, "device").unwrap(), saved);
    assert_eq!(s::snapshot(&mut db).unwrap(), after);
    let mut reused = r.clone();
    reused.content.name = "Other".into();
    assert!(s::save(&mut db, &reused, 11, "device")
        .unwrap_err()
        .contains("OPERATION_REUSED"));
    reused.operation_uuid = Uuid::new_v4().to_string();
    assert!(s::save(&mut db, &reused, 11, "device")
        .unwrap_err()
        .contains("STALE_DRAFT"));
    db.execute("DELETE FROM todos", []).unwrap();
    assert_eq!(s::snapshot(&mut db).unwrap(), after);
    let mut edit = request(Some(saved));
    edit.content.checklist.reverse();
    let updated = s::save(&mut db, &edit, 0, "device").unwrap();
    assert_eq!(updated.updated_at, 11);
    assert!(s::save(&mut db, &r, 99, "device")
        .unwrap_err()
        .contains("STALE_RECEIPT"));
    let mut delete = request(Some(updated.clone()));
    delete.deleted = true;
    let deleted = s::save(&mut db, &delete, 12, "device").unwrap();
    assert_eq!(deleted.deleted_at, Some(12));
    let state = s::snapshot(&mut db).unwrap();
    s::merge(&mut db, &doc(updated)).unwrap();
    assert_eq!(s::snapshot(&mut db).unwrap(), state);
    assert!(s::save(&mut db, &request(Some(deleted)), 13, "device")
        .unwrap_err()
        .contains("DELETED"));
}
#[test]
fn template_storage_limits_validation_conflicts_and_clocks() {
    let mut db = db();
    let mut r = request(None);
    r.content.group_uuid = Some(Uuid::new_v4().to_string());
    assert!(s::save(&mut db, &r, 10, "device")
        .unwrap_err()
        .contains("GROUP_MISSING"));
    r.content.group_uuid = None;
    r.content.checklist = vec!["Step".into(); 21];
    assert!(s::save(&mut db, &r, 10, "device")
        .unwrap_err()
        .contains("CHECKLIST_LIMIT"));
    let mut remote = TemplatesDocument::default();
    for i in 0..101 {
        let mut row = base();
        row.uuid = format!("123e4567-e89b-42d3-a456-{i:012}");
        remote.templates.push(row);
    }
    let state = s::merge(&mut db, &remote).unwrap();
    assert_eq!(state.document.templates.len(), 101);
    assert_eq!(state.synced_revision, 0);
    assert!(s::save(&mut db, &request(None), 11, "device")
        .unwrap_err()
        .contains("TEMPLATE_LIMIT"));
    let mut changed = request(Some(remote.templates[0].clone()));
    changed.content.name = "Renamed".into();
    s::save(&mut db, &changed, 11, "device").unwrap();
    let before = s::snapshot(&mut db).unwrap();
    let mut same_clock = remote.templates[1].clone();
    same_clock.updated_by = "zzz".into();
    same_clock.content.title = "Remote".into();
    s::merge(&mut db, &doc(same_clock)).unwrap();
    assert!(s::save(
        &mut db,
        &request(Some(remote.templates[1].clone())),
        30,
        "device"
    )
    .unwrap_err()
    .contains("STALE_DRAFT"));
    let mut bad = remote.templates[2].clone();
    bad.created_at = 2;
    assert!(s::merge(&mut db, &doc(bad))
        .unwrap_err()
        .contains("IDENTITY_CONFLICT"));
    let mut max = remote.templates[3].clone();
    max.updated_at = crate::task_checklist_protocol::MAX_CLOCK;
    s::merge(&mut db, &doc(max.clone())).unwrap();
    assert!(s::save(&mut db, &request(Some(max)), 1, "device")
        .unwrap_err()
        .contains("CLOCK_OVERFLOW"));
    assert!(s::snapshot(&mut db).unwrap().revision >= before.revision);
}
#[test]
fn template_migration_rolls_back_and_preserves_old_data() {
    let mut db = db();
    db.execute_batch("DROP TABLE task_templates; DROP TABLE task_template_operations; DROP TABLE task_template_sync_state; DELETE FROM schema_migrations WHERE version=21;
        CREATE TRIGGER fail_template_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=21 BEGIN SELECT RAISE(ABORT,'migration failure'); END;").unwrap();
    assert!(crate::db::migrate(&mut db).is_err());
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='task_templates'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    db.execute_batch("DROP TRIGGER fail_template_migration")
        .unwrap();
    crate::db::migrate(&mut db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    assert_eq!(s::snapshot(&mut db).unwrap().revision, 0);
}
