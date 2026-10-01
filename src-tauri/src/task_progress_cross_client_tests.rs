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

// Explicit host benchmark, never run against an application database or cloud target.
#[test]
#[ignore = "requires a temporary performance output path"]
fn task_progress_performance() {
    use crate::recurrence_transport::tests::{Reply, Server};
    use std::{collections::HashSet, sync::Mutex, time::Instant};
    let mut results = Vec::new();
    for rows in [1_000, 10_000, 20_000] {
        let task = Uuid::new_v4().to_string();
        let document = protocol::Document {
            format_version: 1,
            entries: (0..rows)
                .map(|i| protocol::Entry {
                    uuid: Uuid::new_v4().to_string(),
                    task_uuid: task.clone(),
                    body: format!("Synthetic progress {i}"),
                    created_at: i as i64 + 1,
                    created_by: "fixture".into(),
                    updated_at: i as i64 + 1,
                    updated_by: "fixture".into(),
                    clock: 1,
                    deleted_at: None,
                })
                .collect(),
        };
        let raw = protocol::encode(&document).unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::configure_connection(&connection).unwrap();
        crate::db::migrate(&mut connection).unwrap();
        parent(&connection, &task);
        let start = Instant::now();
        store::restore(&mut connection, &document).unwrap();
        let merge_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut page_ms = Vec::new();
        let mut counts_ms = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            assert_eq!(
                store::list(&mut connection, &task, None)
                    .unwrap()
                    .entries
                    .len(),
                30
            );
            page_ms.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            assert_eq!(
                store::counts(&connection, std::slice::from_ref(&task)).unwrap()[0].count,
                rows
            );
            counts_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let start = Instant::now();
        let mut cursor = None;
        let mut seen = HashSet::new();
        let mut pages = 0;
        loop {
            let page = store::list(&mut connection, &task, cursor.as_ref()).unwrap();
            assert_eq!(page.total, rows);
            assert!(page.entries.len() <= 30);
            for entry in page.entries {
                assert!(seen.insert(entry.record.uuid));
            }
            pages += 1;
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen.len(), rows as usize);
        let pagination_ms = start.elapsed().as_secs_f64() * 1000.0;
        let db = crate::db::Database {
            connection: Mutex::new(connection),
        };
        let mut sync_ms = Vec::new();
        tauri::async_runtime::block_on(async {
            for _ in 0..3 {
                // Exclude previous CPU work from the fixture's next-request deadline.
                let server =
                    Server::new(vec![Reply::new(200, Some("\"unchanged\""), raw.as_bytes())]);
                let prepared = crate::s3_sync::PreparedManualSync::from_test_bucket(
                    &db.connection.lock().unwrap(),
                    server.bucket(),
                );
                let start = Instant::now();
                let receipt = crate::task_progress_session::attempt(&db, &prepared)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    crate::task_progress_session::final_token(&db, &receipt)
                        .unwrap()
                        .as_deref(),
                    Some("etag:\"unchanged\"")
                );
                sync_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                assert!(server.request().head.starts_with("GET "));
            }
        });
        page_ms.sort_by(f64::total_cmp);
        counts_ms.sort_by(f64::total_cmp);
        sync_ms.sort_by(f64::total_cmp);
        results.push(
            serde_json::json!({"rows":rows,"bytes":raw.len(),"merge_ms":merge_ms,
            "first_page_median_ms":page_ms[2],"counts_median_ms":counts_ms[2],"pages":pages,
            "pagination_ms":pagination_ms,"unchanged_sync_median_ms":sync_ms[1],"get":3,"put":0}),
        );
        println!("PASS synthetic Rust performance dataset: {rows} rows");
    }
    std::fs::write(
        std::env::var("EGGDONE_PROGRESS_PERF_OUTPUT").unwrap(),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
