//! Text-history restoration through the production sync core and an isolated real S3 peer.
use super::*;
use crate::note_history;

const LOCAL_ONLY: &str = "desktop private history never synchronized";
const HARMONY_TARGET: &str = "harmony restored body";
const DESKTOP_TARGET: &str = "desktop restored body";

fn edit(client: &Client, text: &str) {
    let db = client.db.connection.lock().unwrap();
    let by = crate::db::device_id(&db).unwrap();
    db.execute("UPDATE notes SET title=?1,content=?2,updated_at=max(updated_at+1,?3),updated_by=?4 WHERE uuid=?5",
        params![format!("{text} title"), text, now_millis(), by, NOTE]).unwrap();
}

fn body(client: &Client) -> String {
    client
        .db
        .connection
        .lock()
        .unwrap()
        .query_row("SELECT content FROM notes WHERE uuid=?", [NOTE], |r| {
            r.get(0)
        })
        .unwrap()
}

fn preview(client: &Client, text: &str) -> note_history::HistoryPreview {
    let db = client.db.connection.lock().unwrap();
    let item = note_history::list(&db, NOTE)
        .unwrap()
        .into_iter()
        .find(|h| h.excerpt == text)
        .unwrap();
    note_history::preview(&db, NOTE, item.id).unwrap()
}

fn metadata(client: &Client) -> (String, bool, i64, String, String) {
    let db = client.db.connection.lock().unwrap();
    let (color, pinned, created) = db
        .query_row(
            "SELECT color,pinned,created_at FROM notes WHERE uuid=?",
            [NOTE],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    (
        color,
        pinned,
        created,
        serde_json::to_string(&links::snapshot(&db).unwrap().document).unwrap(),
        serde_json::to_string(&note_attachments::list_active_by_note(&db, NOTE).unwrap()).unwrap(),
    )
}

async fn assert_remote(text: &str) {
    let target = bucket(SECRET);
    let response = target.get_object("account/notes.json").await.unwrap();
    assert_eq!(response.status_code(), 200);
    let wire: serde_json::Value = serde_json::from_slice(response.as_slice()).unwrap();
    assert_eq!(wire["notes"].as_array().unwrap().len(), 1);
    assert_eq!(wire["notes"][0]["content"], text);
    let serialized = String::from_utf8(response.as_slice().to_vec()).unwrap();
    for forbidden in [
        "note_history",
        "captured_at",
        LOCAL_ONLY,
        "harmony unsynchronized private history",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "Unexpected history payload: {forbidden}"
        );
    }
    let pages = target.list("account/".into(), None).await.unwrap();
    let keys: Vec<_> = pages
        .iter()
        .flat_map(|p| p.contents.iter().map(|o| o.key.as_str()))
        .collect();
    for key in &keys {
        assert!(
            [
                "account/todos.json",
                "account/notes.json",
                "account/recurrence-rules.json",
                "account/task-note-links.json",
                "account/note-attachments.json",
                &format!("account/assets/{ASSET}/original")
            ]
            .contains(key),
            "Unexpected object: {key}"
        );
    }
    assert!(keys.contains(&"account/notes.json"));
    assert_eq!(
        target
            .get_object(&format!("account/assets/{ASSET}/original"))
            .await
            .unwrap()
            .as_slice(),
        BYTES
    );
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -NoteHistorySessions with the Harmony peer"]
fn history_session_prepare() {
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
        desktop.add_file();
        edit(&desktop, LOCAL_ONLY);
        edit(&desktop, "desktop published text");
        desktop
            .db
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE notes SET pinned=1,color='blue' WHERE uuid=?",
                [NOTE],
            )
            .unwrap();
        assert_eq!(preview(&desktop, LOCAL_ONLY).entry.text.content, LOCAL_ONLY);
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert!(desktop.state().dirty_domains.is_empty());
        assert_remote("desktop published text").await;
        println!("HISTORY_SESSION_DESKTOP_PREPARE_OK: local-only history, current text and attachment uploaded");
    });
}

#[test]
#[ignore = "Requires Harmony history exchange; never use a user bucket"]
fn history_session_verify() {
    tauri::async_runtime::block_on(async {
        let desktop = Client::new(TODO, NOTE);
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(body(&desktop), HARMONY_TARGET);
        {
            let db = desktop.db.connection.lock().unwrap();
            let items = note_history::list(&db, NOTE).unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].excerpt, "original body");
        }
        // Two independent SQLite clients; the peer remains offline during restoration.
        let stale = Client::new(TODO, NOTE);
        stale.sync(bucket(SECRET)).await.unwrap();
        // Client::new seeds an old active link with its own device ID. Converge that
        // fixture conflict before measuring metadata unchanged by text restoration.
        desktop.sync(bucket(SECRET)).await.unwrap();
        let baseline = metadata(&desktop);
        assert_eq!((&baseline.0, baseline.1), (&"blue".to_string(), true));
        edit(&desktop, DESKTOP_TARGET);
        edit(&desktop, "desktop latest before restore");
        desktop.sync(bucket(SECRET)).await.unwrap();
        let old_preview = preview(&desktop, DESKTOP_TARGET);
        stale.sync(bucket(SECRET)).await.unwrap();
        edit(&stale, "desktop concurrent edit");
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            assert_eq!(
                note_history::restore(&mut db, &old_preview, now_millis(), &by).unwrap_err(),
                "NOTE_HISTORY_CONFLICT"
            );
        }
        assert_eq!(body(&desktop), "desktop concurrent edit");
        let expected = preview(&desktop, DESKTOP_TARGET);
        let stamp;
        {
            let mut db = desktop.db.connection.lock().unwrap();
            let by = crate::db::device_id(&db).unwrap();
            assert!(note_history::restore(&mut db, &expected, now_millis(), &by).unwrap());
            stamp = db
                .query_row("SELECT updated_at FROM notes WHERE uuid=?", [NOTE], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap();
            assert!(stamp > expected.current.updated_at);
        }
        assert_eq!(body(&desktop), DESKTOP_TARGET);
        assert_eq!(metadata(&desktop), baseline);
        assert_eq!(
            preview(&desktop, "desktop concurrent edit")
                .entry
                .text
                .content,
            "desktop concurrent edit"
        );
        assert!(desktop.state().dirty_domains.contains(&"notes".into()));
        assert!(desktop
            .sync(bucket("invalid-fixture-secret"))
            .await
            .is_err());
        assert_eq!(body(&desktop), DESKTOP_TARGET);
        assert!(desktop.state().dirty_domains.contains(&"notes".into()));
        desktop.sync(bucket(SECRET)).await.unwrap();
        stale.sync(bucket(SECRET)).await.unwrap();
        desktop.sync(bucket(SECRET)).await.unwrap();
        assert_eq!(body(&stale), DESKTOP_TARGET);
        assert_eq!(body(&desktop), DESKTOP_TARGET);
        assert_eq!(metadata(&desktop), baseline);
        assert!(desktop.state().dirty_domains.is_empty());
        assert!(stale.state().dirty_domains.is_empty());
        assert_eq!(
            desktop
                .db
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT updated_at FROM notes WHERE uuid=?", [NOTE], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            stamp
        );
        assert_remote(DESKTOP_TARGET).await;
        println!("HISTORY_SESSION_DESKTOP_VERIFY_OK: reverse restore, synchronized stale preview, retained current text, credential retry and stale peer convergence");
    });
}
