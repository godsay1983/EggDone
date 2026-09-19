//! Explicitly opted-in disposable S3 only; no system credentials or user database.
use super::*;
const MAIN: &str = "account/todos.json";
const DESKTOP: &str = "123e4567-e89b-42d3-a456-426614174080";
const HARMONY: &str = "123e4567-e89b-42d3-a456-426614174081";
const DATE: &str = "2026-09-19";

fn configure(c: &Connection) {
    let b = bucket(SECRET);
    let port = std::env::var("EGGDONE_NS7_S3_PORT").unwrap();
    c.execute("UPDATE sync_settings SET enabled=1,endpoint=?1,bucket=?2,region='us-east-1',object_key=?3,path_style=1,allow_http=1", params![format!("http://127.0.0.1:{port}"), b.name, MAIN]).unwrap();
}
async fn follow_and_sync(client: &Client) -> ManualSyncResult {
    let _guard = client.runtime.acquire().unwrap();
    let mut p = s3_sync::prepare_with_fixture_credentials(
        &client.db.connection.lock().unwrap(),
        bucket(SECRET),
    )
    .unwrap();
    if let Some(next) =
        crate::sync_auto_join::follow(&client.db, &client.runtime, &client.assets, &p)
            .await
            .unwrap()
    {
        p = next;
    }
    let result = sync_now_inner(&client.db, &client.runtime, &client.assets, &p, || {})
        .await
        .unwrap();
    p.require_current(&client.db.connection.lock().unwrap())
        .unwrap();
    result
}
#[test]
#[ignore = "Use run-sync-core-s3.ps1 -AutoJoinSessions; disposable bucket only"]
fn prepare() {
    tauri::async_runtime::block_on(async {
        let client = Client::new(DESKTOP, NOTE);
        configure(&client.db.connection.lock().unwrap());
        // Admission first: this peer is already in the ready target when it publishes its new plan.
        follow_and_sync(&client).await;
        {
            let mut c = client.db.connection.lock().unwrap();
            let expected = crate::daily_plan_store::list(&mut c, DATE)
                .unwrap()
                .revision;
            let by = crate::db::device_id(&c).unwrap();
            crate::daily_plan_store::write(
                &mut c,
                &crate::daily_plan_store::DailyPlanWrite {
                    operation_uuid: uuid::Uuid::new_v4().to_string(),
                    task_uuid: DESKTOP.into(),
                    plan_date: DATE.into(),
                    action: "add".into(),
                    expected,
                },
                now_millis(),
                &by,
            )
            .unwrap();
        }
        follow_and_sync(&client).await;
        println!("AUTO_JOIN_DESKTOP_PREPARE_OK");
    });
}
#[test]
#[ignore = "Use run-sync-core-s3.ps1 -AutoJoinSessions after Harmony exchange; disposable bucket only"]
fn verify() {
    tauri::async_runtime::block_on(async {
        // Stale local task has the exact UUID the Harmony peer permanently removed.
        let client = Client::new(TODO, NOTE);
        configure(&client.db.connection.lock().unwrap());
        follow_and_sync(&client).await;
        let mut c = client.db.connection.lock().unwrap();
        assert!(crate::space_activation::is_active(&c).unwrap());
        for id in [DESKTOP, HARMONY] {
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM todos WHERE uuid=?1", [id], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM todos WHERE uuid=?1", [TODO], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM lifecycle_terminals WHERE kind='todo' AND uuid=?1",
                [TODO],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let plan = crate::daily_plan_store::list(&mut c, DATE).unwrap();
        for id in [DESKTOP, HARMONY] {
            assert!(plan
                .current
                .iter()
                .any(|p| p.task_uuid == id && p.status == "planned"));
        }
        assert_eq!(
            c.query_row(
                "SELECT remote_uploaded FROM note_attachments WHERE uuid=?1",
                [ASSET],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        println!("AUTO_JOIN_DESKTOP_VERIFY_OK");
    });
}
