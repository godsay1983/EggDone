use crate::{task_progress_protocol as protocol, task_progress_store as store};
use rusqlite::{params, Connection};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
struct Exchange {
    document: protocol::Document,
    task_uuid: String,
    purged_uuid: String,
    orphan_uuid: String,
}

fn parent(db: &Connection, uuid: &str) {
    db.execute(
        "INSERT INTO todos(uuid,title,completed,created_at,updated_at,updated_by,sort_order) VALUES(?1,'exchange',0,1,1,'fixture',0)",
        [uuid],
    ).unwrap();
}

fn request(
    task: &str,
    record: &str,
    action: &str,
    body: &str,
    expected: Option<String>,
) -> store::ProgressWrite {
    store::ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: task.into(),
        record_uuid: record.into(),
        action: action.into(),
        body: body.into(),
        expected_record: expected,
    }
}

// Invoked by the Harmony host harness; all data stays in isolated SQLite/temp files.
#[test]
#[ignore = "requires a generated Harmony exchange fixture"]
fn task_progress_cross_client_exchange() {
    let input = std::env::var("EGGDONE_PROGRESS_INPUT").unwrap();
    let output = std::env::var("EGGDONE_PROGRESS_OUTPUT").unwrap();
    let exchange: Exchange = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&db).unwrap();
    crate::db::migrate(&mut db).unwrap();
    parent(&db, &exchange.task_uuid);
    parent(&db, &exchange.purged_uuid);
    store::restore(&mut db, &exchange.document).unwrap();
    assert_eq!(
        store::snapshot(&mut db).unwrap().document,
        exchange.document
    );
    assert_eq!(
        store::list(&mut db, &exchange.orphan_uuid, None).unwrap_err(),
        "PROGRESS_UNAVAILABLE"
    );
    parent(&db, &exchange.orphan_uuid);
    assert_eq!(
        store::list(&mut db, &exchange.orphan_uuid, None)
            .unwrap()
            .total,
        1
    );

    let page = store::list(&mut db, &exchange.task_uuid, None).unwrap();
    assert_eq!(page.total, 35);
    assert_eq!(page.entries.len(), 30);
    let last = store::list(&mut db, &exchange.task_uuid, page.next_cursor.as_ref()).unwrap();
    assert_eq!(last.entries.len(), 5);
    assert!(last.next_cursor.is_none());
    let first = &page.entries[0];
    let second = &page.entries[1];
    let edit = request(
        &exchange.task_uuid,
        &first.record.uuid,
        "edit",
        "Desktop edit\nwith two lines",
        Some(first.token.clone()),
    );
    store::write(&mut db, &edit, 2000, "desktop").unwrap();
    assert_eq!(
        store::write(
            &mut db,
            &request(
                &exchange.task_uuid,
                &first.record.uuid,
                "edit",
                "stale",
                Some(first.token.clone())
            ),
            2001,
            "desktop"
        )
        .unwrap_err(),
        "PROGRESS_CONFLICT"
    );
    store::write(
        &mut db,
        &request(
            &exchange.task_uuid,
            &second.record.uuid,
            "delete",
            "",
            Some(second.token.clone()),
        ),
        2002,
        "desktop",
    )
    .unwrap();
    let add = request(
        &exchange.task_uuid,
        &Uuid::new_v4().to_string(),
        "create",
        "Desktop addition",
        None,
    );
    store::write(&mut db, &add, 2003, "desktop").unwrap();
    store::write(&mut db, &add, 9000, "other-device").unwrap();
    assert_eq!(
        store::list(&mut db, &exchange.task_uuid, None)
            .unwrap()
            .total,
        35
    );
    db.execute(
        "UPDATE todos SET completed=1 WHERE uuid=?1",
        [&exchange.task_uuid],
    )
    .unwrap();
    assert!(
        !store::list(&mut db, &exchange.task_uuid, None)
            .unwrap()
            .read_only
    );
    db.execute(
        "UPDATE todos SET archived_at=10 WHERE uuid=?1",
        [&exchange.task_uuid],
    )
    .unwrap();
    assert!(
        store::list(&mut db, &exchange.task_uuid, None)
            .unwrap()
            .read_only
    );
    assert_eq!(
        store::write(
            &mut db,
            &request(
                &exchange.task_uuid,
                &Uuid::new_v4().to_string(),
                "create",
                "blocked",
                None
            ),
            2004,
            "desktop"
        )
        .unwrap_err(),
        "PROGRESS_READ_ONLY"
    );
    db.execute(
        "UPDATE todos SET archived_at=NULL WHERE uuid=?1",
        [&exchange.task_uuid],
    )
    .unwrap();

    db.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,3000)", params![exchange.purged_uuid, Uuid::new_v4().to_string()]).unwrap();
    store::restore(&mut db, &exchange.document).unwrap();
    let returned = store::snapshot(&mut db).unwrap().document;
    assert!(returned
        .entries
        .iter()
        .all(|e| e.task_uuid != exchange.purged_uuid));
    let deleted = returned
        .entries
        .iter()
        .find(|e| e.uuid == second.record.uuid)
        .unwrap();
    assert_eq!(deleted.body, "");
    assert!(deleted.deleted_at.is_some());
    assert_eq!(
        returned
            .entries
            .iter()
            .find(|e| e.uuid == first.record.uuid)
            .unwrap()
            .body,
        "Desktop edit\nwith two lines"
    );
    let counts = store::counts(
        &db,
        &[
            exchange.task_uuid,
            exchange.purged_uuid,
            exchange.orphan_uuid,
        ],
    )
    .unwrap();
    assert_eq!(
        counts.iter().map(|e| e.count).collect::<Vec<_>>(),
        vec![35, 0, 1]
    );
    std::fs::write(output, protocol::encode(&returned).unwrap()).unwrap();
}
