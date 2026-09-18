//! Production archive and full sync sessions against disposable S3, not native UI acceptance.
use super::*;
use crate::archive as archive_store;

const REVERSE_UNARCHIVE: &str = "123e4567-e89b-42d3-a456-426614174030";
const REVERSE_REOPEN: &str = "123e4567-e89b-42d3-a456-426614174031";
const CHECK: &str = "123e4567-e89b-42d3-a456-426614174040";

fn row(client: &Client, id: &str) -> crate::sync::SyncTodo {
    crate::sync::build_document(&client.db.connection.lock().unwrap(), now_millis())
        .unwrap()
        .todos
        .into_iter()
        .find(|t| t.uuid == id)
        .unwrap()
}

fn assert_restored(client: &Client, id: &str, completed: bool) {
    let task = row(client, id);
    assert_eq!(task.completed, completed);
    assert_eq!(task.completed_at, completed.then_some(100));
    assert_eq!(task.archived_at, None);
    assert_eq!(task.deleted_at, None);
    assert_eq!(task.reminder_at, None);
    assert_eq!(task.due_date.as_deref(), Some("2026-09-13"));
    assert_eq!(task.repeat_rule.as_deref(), completed.then_some("daily"));
    assert_eq!(
        task.repeat_next_due_date.as_deref(),
        completed.then_some("2026-09-14")
    );
    if !completed {
        assert_eq!(task.repeat_series_uuid, None);
    }
}

fn assert_no_successor(client: &Client) {
    let db = client.db.connection.lock().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM todos", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM recurrence_rules", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let record: String = db
        .query_row(
            "SELECT record_json FROM task_checklist_items WHERE uuid=?",
            [CHECK],
            |r| r.get(0),
        )
        .unwrap();
    let check: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert_eq!(check["content"], "Preserve checked item");
    assert_eq!(check["completed"], true);
    assert!(links::snapshot(&db)
        .unwrap()
        .document
        .links
        .iter()
        .all(|l| l.deleted_at.is_none()));
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -ArchiveRecoverySessions with the Harmony peer"]
fn archive_session_prepare() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let response = Bucket::create_with_path_style(
            &target.name,
            target.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .unwrap();
        assert_eq!(response.response_code, 200);
        let desktop = Client::new(TODO, NOTE);
        {
            let db = desktop.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            db.execute("INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by) VALUES(?1,'Desktop reopen',1,100,100,?2)", params![TODO2, by]).unwrap();
            db.execute("UPDATE todos SET completed=1,completed_at=100,due_date='2026-09-13',reminder_at=9999999999999,repeat_rule='daily',repeat_next_due_date='2026-09-14'", []).unwrap();
            let record = serde_json::json!({"uuid":CHECK,"todo_uuid":TODO,"source_rule_uuid":null,
                "source_entry_uuid":null,"content":"Preserve checked item","sort_order":1000,
                "completed":true,"created_at":100,"updated_at":100,"updated_by":by,"deleted_at":null});
            db.execute("INSERT INTO task_checklist_items(uuid,todo_uuid,active,record_json) VALUES(?1,?2,1,?3)", params![CHECK, TODO, record.to_string()]).unwrap();
            assert_eq!(archive_completed_todos_in_connection(&db).unwrap(), 2);
        }
        desktop.sync(bucket(SECRET)).await.unwrap();
        for id in [TODO, TODO2] {
            assert!(row(&desktop, id).archived_at.is_some());
        }
        assert!(desktop.state().dirty_domains.is_empty());
        println!("ARCHIVE_SESSION_DESKTOP_PREPARE_OK: production archive, legacy repeat history, checklist and links uploaded");
    });
}

#[test]
#[ignore = "Requires Harmony archive exchange; never use a user bucket"]
fn archive_session_verify() {
    tauri::async_runtime::block_on(async {
        // This peer has an obsolete active task. Remote restoration must win on reconnect.
        let desktop = Client::new(TODO, NOTE);
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert_restored(&desktop, TODO, true);
        assert_restored(&desktop, TODO2, false);
        assert_no_successor(&desktop);
        let stale = Client::new(TODO, NOTE);
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();

        let expected = {
            let mut db = desktop.db.connection.lock().unwrap();
            archive_store::preview(&mut db, REVERSE_REOPEN)
                .unwrap()
                .expected
        };
        {
            let db = stale.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            db.execute("UPDATE todos SET title='Concurrent archived edit',updated_at=max(updated_at+1,?1),updated_by=?2 WHERE uuid=?3", params![now_millis(),by,REVERSE_REOPEN]).unwrap();
        }
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            assert_eq!(
                archive_store::apply(
                    &mut db,
                    &uuid::Uuid::new_v4().to_string(),
                    archive_store::Action::Reopen,
                    &expected,
                    now_millis(),
                    &by
                )
                .unwrap_err(),
                "ARCHIVE_CONFLICT"
            );
            for (id, action) in [
                (REVERSE_UNARCHIVE, archive_store::Action::Unarchive),
                (REVERSE_REOPEN, archive_store::Action::Reopen),
            ] {
                let preview = archive_store::preview(&mut db, id).unwrap();
                let op = uuid::Uuid::new_v4().to_string();
                archive_store::apply(&mut db, &op, action, &preview.expected, now_millis(), &by)
                    .unwrap();
                assert_eq!(
                    archive_store::apply(
                        &mut db,
                        &op,
                        action,
                        &preview.expected,
                        now_millis(),
                        &by
                    )
                    .unwrap()
                    .outcome,
                    "already_applied"
                );
            }
        }
        let before = desktop.state().dirty_domains;
        assert!(!before.is_empty());
        assert!(desktop
            .sync(bucket("intentionally-invalid-fixture"))
            .await
            .is_err());
        assert_eq!(desktop.state().dirty_domains, before);
        desktop.sync(bucket(SECRET)).await.unwrap();
        // The stale client still holds archived copies. Sync must not undo the restoration.
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();
        for client in [&desktop, &stale] {
            assert_restored(client, REVERSE_UNARCHIVE, true);
            assert_restored(client, REVERSE_REOPEN, false);
            assert_eq!(
                row(client, REVERSE_REOPEN).title,
                "Concurrent archived edit"
            );
            assert_no_successor(client);
            assert!(client.state().dirty_domains.is_empty());
        }
        println!("ARCHIVE_SESSION_DESKTOP_VERIFY_OK: reverse restore, conflict guard, duplicate retry, rejected credentials and stale archive reconnect");
    });
}
