use super::*;
use crate::{task_note_link_protocol::LinkDocument, task_note_link_store as store};
use rusqlite::params;
const TODO: &str = "123e4567-e89b-42d3-a456-426614174000";
const NOTE: &str = "123e4567-e89b-42d3-a456-426614174001";

fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    store::merge(
        &mut db,
        &LinkDocument {
            format_version: 1,
            links: vec![TaskNoteLink {
                uuid: link_uuid(TODO, NOTE).unwrap(),
                todo_uuid: TODO.into(),
                note_uuid: NOTE.into(),
                created_at: 1,
                updated_at: 1,
                updated_by: "fixture".into(),
                deleted_at: None,
            }],
        },
    )
    .unwrap();
    db
}

#[test]
fn shared_view_states_are_scoped_read_only_and_keep_dangling_links() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-note-link-views-v1.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let db = fixture();
        let todo = case["todo"].as_str().unwrap();
        let note = case["note"].as_str().unwrap();
        if todo != "missing" {
            db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by,completed,archived_at,deleted_at,repeat_series_uuid)
              VALUES(?1,'task',0,1,1,'fixture',?2,?3,?4,?5)", params![TODO, todo == "completed" || todo == "archived",
                if todo == "archived" { Some(2) } else { None }, if todo == "deleted" { Some(2) } else { None },
                if case["repeating"] == true { Some(TODO) } else { None }]).unwrap();
        }
        if note != "missing" {
            db.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by,deleted_at)
              VALUES(?1,'note','body','default',0,1,1,'fixture',?2)", params![NOTE, if note == "deleted" { Some(2) } else { None }]).unwrap();
        }
        let before = db.total_changes();
        let rows = list(&db, LinkScope::Todo, &TODO.to_uppercase()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(list(&db, LinkScope::Note, NOTE).unwrap(), rows);
        let view = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(view["todo_state"], case["todo"]);
        assert_eq!(view["note_state"], case["note"]);
        assert_eq!(view["is_repeating"], case["repeating"]);
        assert_eq!(
            rows[0].todo_title.is_none(),
            ["missing", "deleted"].contains(&todo)
        );
        assert_eq!(
            rows[0].note_title.is_none(),
            ["missing", "deleted"].contains(&note)
        );
        assert_eq!(pair(&db, TODO, NOTE).unwrap().as_ref(), Some(&rows[0].link));
        assert!(list(&db, LinkScope::Todo, NOTE).unwrap().is_empty());
        assert_eq!(db.total_changes(), before);
    }
}

#[test]
fn views_preserve_tombstone_for_relink_and_reject_corrupt_rows() {
    let mut db = fixture();
    let current = pair(&db, TODO, NOTE).unwrap().unwrap();
    let removed = crate::task_note_link_operations::change(
        &mut db,
        TODO,
        NOTE,
        false,
        Some(&current),
        2,
        "fixture",
    )
    .unwrap();
    assert!(list(&db, LinkScope::Note, NOTE).unwrap().is_empty());
    assert_eq!(pair(&db, TODO, NOTE).unwrap(), removed);
    assert!(list(&db, LinkScope::Todo, "' OR 1=1 --").is_err());
    assert!(pair(&db, "invalid", NOTE).is_err());
    assert!(serde_json::from_str::<LinkScope>("\"invalid\"").is_err());
    db.execute("UPDATE task_note_links SET active=1,record_json=''", [])
        .unwrap();
    assert!(list(&db, LinkScope::Note, NOTE).is_err());
    assert!(pair(&db, TODO, NOTE).is_err());
    let key = link_uuid(TODO, NOTE).unwrap();
    let other = TaskNoteLink {
        uuid: link_uuid(NOTE, TODO).unwrap(),
        todo_uuid: NOTE.into(),
        note_uuid: TODO.into(),
        ..current
    };
    db.execute(
        "UPDATE task_note_links SET record_json=?1 WHERE uuid=?2",
        params![serde_json::to_string(&other).unwrap(), key],
    )
    .unwrap();
    assert!(list(&db, LinkScope::Note, NOTE).is_err());
    assert!(pair(&db, TODO, NOTE).is_err());
}

#[test]
fn view_order_uses_creation_clock_then_uuid_not_entity_title_or_db_order() {
    let mut db = fixture();
    let first = pair(&db, TODO, NOTE).unwrap().unwrap();
    let mut second = first.clone();
    second.note_uuid = "123e4567-e89b-42d3-a456-426614174002".into();
    second.uuid = link_uuid(TODO, &second.note_uuid).unwrap();
    second.created_at = 0;
    store::merge(
        &mut db,
        &LinkDocument {
            format_version: 1,
            links: vec![second.clone()],
        },
    )
    .unwrap();
    assert_eq!(
        list(&db, LinkScope::Todo, TODO)
            .unwrap()
            .iter()
            .map(|v| &v.link)
            .collect::<Vec<_>>(),
        vec![&second, &first]
    );
    let mut third = first.clone();
    third.note_uuid = "123e4567-e89b-42d3-a456-426614174003".into();
    third.uuid = link_uuid(TODO, &third.note_uuid).unwrap();
    store::merge(
        &mut db,
        &LinkDocument {
            format_version: 1,
            links: vec![third.clone()],
        },
    )
    .unwrap();
    let mut tied = vec![first, third];
    tied.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    let ordered = list(&db, LinkScope::Todo, TODO).unwrap();
    assert_eq!(ordered[0].link, second);
    assert_eq!(
        ordered[1..].iter().map(|v| &v.link).collect::<Vec<_>>(),
        tied.iter().collect::<Vec<_>>()
    );
}
