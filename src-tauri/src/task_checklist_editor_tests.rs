use crate::task_checklist_editor::*;
use crate::task_checklist_protocol::*;
use crate::task_checklist_store as checklist;
use crate::{
    recurrence_protocol::{RecurrenceDocument, RecurrenceRule},
    recurrence_store,
};
use rusqlite::{params, Connection};
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/task-checklist-editor-v1.json"
    ))
    .unwrap()
}
fn text(f: &Value, key: &str) -> String {
    f[key].as_str().unwrap().into()
}
fn setup(active: bool) -> (Connection, EditorRequest, String) {
    let f = fixture();
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    let todo = text(&f, "todo_uuid");
    let by = text(&f, "by");
    db.execute("INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,updated_by,due_date,reminder_at) VALUES(?1,'original','body',0,1,10,?2,'2026-09-20',5000)",params![todo,by]).unwrap();
    let mut expected = RecurrenceDocument {
        format_version: 1,
        rules: vec![],
    };
    if active {
        let rule = RecurrenceRule {
            uuid: text(&f, "rule_uuid"),
            first_todo_uuid: todo.clone(),
            schedule: serde_json::from_value(f["schedule"].clone()).unwrap(),
            timezone_id: None,
            current_todo_uuid: todo.clone(),
            current_date: "2026-09-20".into(),
            generated_count: 1,
            exhausted: false,
            updated_at: 10,
            updated_by: by.clone(),
            deleted_at: None,
        };
        expected.rules.push(rule);
        recurrence_store::merge(&mut db, &expected).unwrap();
        db.execute("UPDATE todos SET repeat_series_uuid=?1", [&todo])
            .unwrap();
    }
    let request = EditorRequest {
        task: checklist::ChecklistSave {
            operation_uuid: text(&f, "operation_uuid"),
            todo_uuid: todo,
            expected_updated_at: 10,
            expected_items: ItemsDocument::default(),
            title: "edited".into(),
            note: "new body".into(),
            items: vec![checklist::ChecklistEdit {
                uuid: text(&f, "item_uuid"),
                content: "step".into(),
                sort_order: 0,
                completed: false,
            }],
        },
        fields: serde_json::from_value(f["fields"].clone()).unwrap(),
        expected_rules: expected,
        mode: "keep".into(),
        replaces_uuid: None,
        replacement: None,
        future_entries: vec![],
    };
    (db, request, by)
}
fn replacement(r: &mut EditorRequest) {
    let f = fixture();
    r.mode = "replace".into();
    r.replaces_uuid = r.expected_rules.rules.first().map(|r| r.uuid.clone());
    r.replacement = Some(RuleSeed {
        uuid: text(&f, "replacement_uuid"),
        schedule: serde_json::from_value(f["schedule"].clone()).unwrap(),
        timezone_id: None,
    });
    r.future_entries = vec![DefinitionEntry {
        uuid: text(&f, "entry_uuid"),
        content: "future step".into(),
        sort_order: 0,
    }];
}

fn request_from_editor_snapshot(
    s: crate::task_checklist_views::ChecklistEditorSnapshot,
) -> EditorRequest {
    EditorRequest {
        task: checklist::ChecklistSave {
            operation_uuid: uuid::Uuid::new_v4().to_string(),
            todo_uuid: s.task.todo_uuid,
            expected_updated_at: s.task.updated_at,
            expected_items: s.task.items.clone(),
            title: s.task.title,
            note: s.task.note,
            items: s
                .task
                .items
                .items
                .into_iter()
                .filter(|i| i.deleted_at.is_none())
                .map(|i| checklist::ChecklistEdit {
                    uuid: i.uuid,
                    content: i.content,
                    sort_order: i.sort_order,
                    completed: i.completed,
                })
                .collect(),
        },
        fields: s.fields,
        expected_rules: s.rules,
        mode: "keep".into(),
        replaces_uuid: None,
        replacement: None,
        future_entries: vec![],
    }
}

#[test]
fn checklist_editor_snapshot_roundtrip_preserves_fields_and_future_definition() {
    let (mut db, mut r, by) = setup(true);
    replacement(&mut r);
    let first = save(&mut db, &r, 100, &by).unwrap();
    let s = crate::task_checklist_views::read_editor(&mut db, &r.task.todo_uuid).unwrap();
    assert_eq!(s.fields, r.fields);
    assert_eq!(s.task.updated_at, first.updated_at);
    assert_eq!(s.rules.rules.len(), 2);
    assert_eq!(
        s.definitions.definitions[0].entries[0].content,
        "future step"
    );
    assert!(!s.completed);
    assert!(!s.task.read_only);
    assert_eq!(
        s.repeat_series_uuid.as_deref(),
        Some(r.task.todo_uuid.as_str())
    );
    let request = request_from_editor_snapshot(s);
    let result = save(&mut db, &request, 9000, &by).unwrap();
    assert_eq!(result.updated_at, first.updated_at);
    assert!(!result.reminder_changed);
    let next = crate::task_checklist_views::read_editor(&mut db, &r.task.todo_uuid).unwrap();
    assert_eq!(next.fields.reminder_at, Some(5000));
    assert_eq!(next.definitions.definitions.len(), 1);
}

#[test]
fn checklist_editor_snapshot_exposes_readonly_and_rejects_deleted_missing_corrupt() {
    let (mut db, r, _) = setup(true);
    let id = &r.task.todo_uuid;
    db.execute(
        "UPDATE todos SET completed=1,archived_at=20 WHERE uuid=?1",
        [id],
    )
    .unwrap();
    let s = crate::task_checklist_views::read_editor(&mut db, id).unwrap();
    assert!(s.completed && s.task.read_only);
    db.execute("UPDATE todos SET deleted_at=30 WHERE uuid=?1", [id])
        .unwrap();
    assert!(crate::task_checklist_views::read_editor(&mut db, id)
        .unwrap_err()
        .contains("PARENT_MISSING"));
    assert!(crate::task_checklist_views::read_editor(&mut db, "invalid").is_err());
    assert!(
        crate::task_checklist_views::read_editor(&mut db, &uuid::Uuid::new_v4().to_string())
            .is_err()
    );
    db.execute("UPDATE todos SET deleted_at=NULL WHERE uuid=?1", [id])
        .unwrap();
    db.execute("UPDATE recurrence_rules SET record_json='broken'", [])
        .unwrap();
    assert!(crate::task_checklist_views::read_editor(&mut db, id).is_err());
}

#[test]
fn checklist_editor_snapshot_detects_parent_rule_and_child_conflicts() {
    for domain in ["parent", "rule", "child"] {
        let (mut db, r, by) = setup(true);
        save(&mut db, &r, 100, &by).unwrap();
        let s = crate::task_checklist_views::read_editor(&mut db, &r.task.todo_uuid).unwrap();
        let mut request = request_from_editor_snapshot(s);
        request.task.title = "stale draft".into();
        match domain {
            "parent" => {
                db.execute(
                    "UPDATE todos SET title='remote',updated_at=updated_at+1",
                    [],
                )
                .unwrap();
            }
            "rule" => {
                let mut rules = request.expected_rules.clone();
                rules.rules[0].updated_at += 1;
                recurrence_store::merge(&mut db, &rules).unwrap();
            }
            _ => {
                let mut items = request.task.expected_items.clone();
                items.items[0].completed = true;
                items.items[0].updated_at += 1;
                checklist::merge(&mut db, &items, &DefinitionsDocument::default()).unwrap();
            }
        }
        let before = dump(&db);
        assert!(save(&mut db, &request, 200, &by).is_err(), "{domain}");
        assert_eq!(dump(&db), before, "{domain}");
    }
}
fn dump(db: &Connection) -> String {
    let mut out = vec![];
    for table in [
        "todos",
        "recurrence_rules",
        "recurrence_sync_state",
        "task_checklist_items",
        "task_checklist_definitions",
        "task_checklist_sync_state",
        "task_checklist_operations",
        "app_metadata",
    ] {
        let mut stmt = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            out.push(format!("{table}:{row:?}"));
        }
    }
    out.join("\n")
}
#[test]
fn checklist_editor_full_fields_preserve_reminder_and_replay() {
    let (mut db, mut r, by) = setup(false);
    r.fields.priority = 1;
    r.fields.reminder_at = 6000.into();
    r.fields.due_date = None;
    r.fields.due_at = 2000.into();
    let result = save(&mut db, &r, 100, &by).unwrap();
    assert!(result.reminder_changed);
    assert_eq!(
        db.query_row("SELECT priority FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let before = dump(&db);
    assert_eq!(save(&mut db, &r, 10000, &by).unwrap(), result);
    assert_eq!(dump(&db), before);
    r.fields.reminder_at = 7000.into();
    assert!(save(&mut db, &r, 100, &by)
        .unwrap_err()
        .contains("OPERATION_REUSED"));
    assert_eq!(dump(&db), before);
}
#[test]
fn checklist_editor_same_schedule_forks_and_stop_keeps_definition() {
    let (mut db, mut r, by) = setup(true);
    replacement(&mut r);
    let result = save(&mut db, &r, 100, &by).unwrap();
    assert!(!result.reminder_changed);
    assert_eq!(
        db.query_row("SELECT reminder_at FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
    let s = checklist::snapshot(&mut db).unwrap();
    assert_eq!(s.definitions.definitions.len(), 1);
    assert_eq!(s.items.items[0].source_rule_uuid, None);
    let rules = recurrence_store::snapshot(&db).unwrap().document;
    assert_eq!(
        rules
            .rules
            .iter()
            .filter(|r| r.deleted_at.is_some())
            .count(),
        1
    );
    let before = dump(&db);
    save(&mut db, &r, 500, &by).unwrap();
    assert_eq!(dump(&db), before);
    r.mode = "stop".into();
    r.task.operation_uuid = uuid::Uuid::new_v4().to_string();
    r.task.expected_updated_at = result.updated_at;
    r.task.expected_items = s.items;
    r.expected_rules = rules;
    r.replaces_uuid = result.rule_uuid;
    r.replacement = None;
    r.future_entries.clear();
    assert!(save(&mut db, &r, 200, &by).unwrap().rule_uuid.is_none());
    assert_eq!(
        checklist::snapshot(&mut db)
            .unwrap()
            .definitions
            .definitions
            .len(),
        1
    );
    assert!(db
        .query_row("SELECT repeat_series_uuid FROM todos", [], |r| r
            .get::<_, Option<String>>(0))
        .unwrap()
        .is_none());
}
#[test]
fn checklist_editor_every_write_failure_rolls_back() {
    for trigger in fixture()["failure_triggers"].as_array().unwrap() {
        let (mut db, mut r, by) = setup(true);
        replacement(&mut r);
        let before = dump(&db);
        db.execute_batch(&format!(
            "CREATE TRIGGER injected {} BEGIN SELECT RAISE(ABORT,'injected'); END;",
            trigger.as_str().unwrap()
        ))
        .unwrap();
        assert!(
            save(&mut db, &r, 100, &by)
                .unwrap_err()
                .contains("injected"),
            "{trigger}"
        );
        assert_eq!(dump(&db), before, "{trigger}");
        db.execute_batch("DROP TRIGGER injected").unwrap();
        save(&mut db, &r, 100, &by).unwrap();
    }
}
#[test]
fn checklist_editor_invalid_and_stale_never_partially_save() {
    for case in fixture()["invalid_fields"].as_array().unwrap() {
        let (mut db, mut r, by) = setup(false);
        let before = dump(&db);
        let mut f = serde_json::to_value(&r.fields).unwrap();
        f[case["field"].as_str().unwrap()] = case["value"].clone();
        r.fields = serde_json::from_value(f).unwrap();
        assert!(save(&mut db, &r, 100, &by).is_err(), "{}", case["name"]);
        assert_eq!(dump(&db), before);
    }
    let (mut db, mut r, by) = setup(true);
    replacement(&mut r);
    let mut changed = r.expected_rules.clone();
    changed.rules[0].updated_at += 1;
    recurrence_store::merge(&mut db, &changed).unwrap();
    let before = dump(&db);
    assert!(save(&mut db, &r, 100, &by)
        .unwrap_err()
        .contains("RULE_STALE"));
    assert_eq!(dump(&db), before);
}
#[test]
fn checklist_editor_legacy_conversion_timed_and_current_only() {
    let (mut db, mut r, by) = setup(false);
    db.execute_batch("UPDATE todos SET repeat_rule='daily',repeat_series_uuid=uuid")
        .unwrap();
    replacement(&mut r);
    let seed = r.replacement.as_mut().unwrap();
    seed.schedule.local_time_minutes = Some(600);
    seed.timezone_id = Some("Asia/Shanghai".into());
    r.fields.due_date = None;
    r.fields.due_at =
        crate::recurrence_time::recurrence_due_at("2026-09-20", Some(600), Some("Asia/Shanghai"))
            .unwrap();
    assert_eq!(
        r.fields.due_at,
        fixture()["expected"]["timed_due_at"].as_i64()
    );
    save(&mut db, &r, 100, &by).unwrap();
    assert!(db
        .query_row("SELECT repeat_rule FROM todos", [], |r| r
            .get::<_, Option<String>>(0))
        .unwrap()
        .is_none());
    let (mut db, r, by) = setup(true);
    let rules = recurrence_store::snapshot(&db).unwrap().document;
    save(&mut db, &r, 100, &by).unwrap();
    assert_eq!(recurrence_store::snapshot(&db).unwrap().document, rules);
    assert!(checklist::snapshot(&mut db)
        .unwrap()
        .definitions
        .definitions
        .is_empty());
}

#[test]
fn checklist_editor_noop_and_readonly_guards() {
    let (mut db, mut r, by) = setup(true);
    r.task.title = "original".into();
    r.task.note = "body".into();
    r.task.items.clear();
    let before = checklist::snapshot(&mut db).unwrap();
    assert_eq!(save(&mut db, &r, 100, &by).unwrap().updated_at, 10);
    assert_eq!(checklist::snapshot(&mut db).unwrap(), before);
    for sql in [
        "UPDATE todos SET deleted_at=20",
        "UPDATE todos SET archived_at=20",
        "UPDATE todos SET updated_at=11",
    ] {
        let (mut db, r, by) = setup(false);
        db.execute_batch(sql).unwrap();
        let before = dump(&db);
        assert!(save(&mut db, &r, 100, &by).is_err());
        assert_eq!(dump(&db), before);
    }
    let (mut db, mut r, by) = setup(true);
    replacement(&mut r);
    r.replacement.as_mut().unwrap().uuid = fixture()["rule_uuid"].as_str().unwrap().into();
    let before = dump(&db);
    assert!(save(&mut db, &r, 100, &by).is_err());
    assert_eq!(dump(&db), before);
    let (mut db, mut r, by) = setup(true);
    replacement(&mut r);
    r.fields.due_date = Some("2026-09-21".into());
    let before = dump(&db);
    assert!(save(&mut db, &r, 100, &by)
        .unwrap_err()
        .contains("SCHEDULE_MISMATCH"));
    assert_eq!(dump(&db), before);
}

#[test]
fn checklist_editor_later_instance_keeps_history() {
    let (mut db, mut r, by) = setup(true);
    let old_todo = r.task.todo_uuid.clone();
    let mut rule = r.expected_rules.rules[0].clone();
    let occurrence = crate::recurrence::RecurrenceOccurrence {
        date: "2026-09-21".into(),
        index: 2,
    };
    let todo =
        crate::recurrence::recurrence_todo_uuid(&rule.uuid, &rule.schedule, &occurrence).unwrap();
    rule.current_todo_uuid = todo.clone();
    rule.current_date = occurrence.date;
    rule.generated_count = 2;
    rule.updated_at = 20;
    r.expected_rules.rules = vec![rule];
    recurrence_store::merge(&mut db, &r.expected_rules).unwrap();
    db.execute("INSERT INTO todos(uuid,title,note,sort_order,created_at,updated_at,updated_by,due_date,reminder_at,repeat_series_uuid) VALUES(?1,'original','body',0,1,10,?2,'2026-09-21',5000,?3)",params![todo,by,old_todo]).unwrap();
    r.task.todo_uuid = todo.clone();
    r.fields.due_date = Some("2026-09-21".into());
    replacement(&mut r);
    r.replacement.as_mut().unwrap().schedule.anchor_date = "2026-09-21".into();
    save(&mut db, &r, 100, &by).unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM todos WHERE uuid=?1", [old_todo], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "original"
    );
    assert_eq!(
        checklist::snapshot(&mut db)
            .unwrap()
            .definitions
            .definitions[0]
            .first_todo_uuid,
        todo
    );
}

#[test]
fn checklist_editor_orphan_receipt_cannot_bypass_stale_parent() {
    let (mut db, r, by) = setup(false);
    let id = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_DNS,
        format!("eggdone:checklist-editor-body:v1:{}", r.task.operation_uuid).as_bytes(),
    )
    .to_string();
    db.execute("INSERT INTO task_checklist_operations(operation_uuid,payload,result_updated_at) VALUES(?1,'orphan',100)",[id]).unwrap();
    let before = dump(&db);
    assert!(save(&mut db, &r, 100, &by)
        .unwrap_err()
        .contains("RECEIPT_CONFLICT"));
    assert_eq!(dump(&db), before);
}
