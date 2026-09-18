//! The unchanged pre-purge production core is the legacy peer, not a released GUI binary.
use super::*;

const CHECKPOINT: &str =
    "account/lifecycle-prototype/v1/123e4567-e89b-42d3-a456-426614174070/checkpoint.json";

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -PurgeCompatibilitySessions"]
fn legacy_prepare() {
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
        let client = Client::new(TODO, NOTE);
        client
            .db
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE todos SET deleted_at=200,updated_at=200 WHERE uuid=?",
                [TODO],
            )
            .unwrap();
        client.sync(bucket(SECRET)).await.unwrap();
        println!("PURGE_COMPAT_DESKTOP_PREPARE_OK: legacy deletion uploaded to isolated old key");
    });
}

#[test]
#[ignore = "Requires the Harmony purge prototype exchange phase"]
fn legacy_late_write() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let before = target.get_object(CHECKPOINT).await.unwrap();
        assert_eq!(before.status_code(), 200);
        let checkpoint: serde_json::Value = serde_json::from_slice(before.as_slice()).unwrap();
        assert_eq!(checkpoint["records"].as_array().unwrap().len(), 0);
        assert_eq!(checkpoint["terminals"].as_array().unwrap().len(), 1);
        let client = Client::new(TODO, NOTE);
        client.sync(bucket(SECRET)).await.unwrap();
        {
            let db = client.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            db.execute("UPDATE todos SET title='Late legacy desktop edit',deleted_at=NULL,updated_at=max(updated_at+1,?1),updated_by=?2 WHERE uuid=?3",
                params![now_millis()+100000,by,TODO]).unwrap();
        }
        client.sync(bucket(SECRET)).await.unwrap();
        let old = target.get_object("account/todos.json").await.unwrap();
        let wire: serde_json::Value = serde_json::from_slice(old.as_slice()).unwrap();
        assert!(
            wire.get("purged_entities").is_none(),
            "legacy serialization drops unknown terminal field"
        );
        assert_eq!(wire["todos"][0]["title"], "Late legacy desktop edit");
        assert!(wire["todos"][0]["deleted_at"].is_null());
        let after = target.get_object(CHECKPOINT).await.unwrap();
        assert_eq!(
            after.as_slice(),
            before.as_slice(),
            "old peer must not write the new checkpoint key"
        );
        println!("PURGE_COMPAT_DESKTOP_LATE_OK: unknown field lost, old entity restored, isolated checkpoint unchanged");
    });
}
