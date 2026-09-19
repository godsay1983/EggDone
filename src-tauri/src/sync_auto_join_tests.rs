use super::*;
use crate::migration_backup::{cloud, publication, FileEntry};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
const MAIN: &str = "account/todos.json";
const ID: &str = "123e4567-e89b-42d3-a456-426614174000";
const PEER: &str = "123e4567-e89b-42d3-a456-426614174001";
const ASSET: &str = "123e4567-e89b-42d3-a456-426614174020";
const DATE: &str = "2026-09-19";

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    binaries: BTreeMap<(String, String, String), Vec<u8>>,
    local: BTreeMap<String, Vec<u8>>,
    reads: Vec<String>,
    downloads: usize,
    uploads: usize,
    fail: Option<String>,
    lose_upload: bool,
    on_read: Option<Box<dyn FnOnce() + Send>>,
}
#[derive(Default)]
struct Cloud(Mutex<Memory>);
impl Io for Cloud {
    async fn read(
        &self,
        _: &PreparedManualSync,
        key: &str,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        let mut c = self.0.lock().unwrap();
        c.reads.push(key.into());
        if let Some(hook) = c.on_read.take() {
            hook();
        }
        if c.fail.as_deref() == Some(key) {
            return Err("SYNC_AUTO_JOIN_DENIED".into());
        }
        let value = c.objects.get(key).cloned();
        assert!(value.as_ref().is_none_or(|b| b.len() <= limit));
        Ok(value)
    }
    async fn head(&self, p: &PreparedManualSync, a: &Asset) -> Result<RemoteAssetState, String> {
        let c = self.0.lock().unwrap();
        let b = c
            .binaries
            .get(&(p.main_key().into(), a.uuid.clone(), a.name.clone()));
        Ok(RemoteAssetState {
            exists: b.is_some(),
            etag: b.map(|_| "\"binary\"".into()),
            content_length: b.map(|b| b.len() as i64),
            content_type: Some(a.mime.clone()),
            sha256: b.map(|b| space::hash(b)),
        })
    }
    async fn download(&self, p: &PreparedManualSync, a: &Asset) -> Result<Vec<u8>, String> {
        let mut c = self.0.lock().unwrap();
        c.downloads += 1;
        c.binaries
            .get(&(p.main_key().into(), a.uuid.clone(), a.name.clone()))
            .cloned()
            .ok_or("SYNC_AUTO_JOIN_ASSET_MISSING".into())
    }
    async fn upload(&self, p: &PreparedManualSync, a: &Asset, bytes: &[u8]) -> Result<(), String> {
        let mut c = self.0.lock().unwrap();
        c.uploads += 1;
        let saved = c
            .binaries
            .entry((p.main_key().into(), a.uuid.clone(), a.name.clone()))
            .or_insert_with(|| bytes.to_vec());
        if saved.as_slice() != bytes {
            return Err("SYNC_AUTO_JOIN_ASSET_MISMATCH".into());
        }
        if c.lose_upload {
            c.lose_upload = false;
            return Err("SYNC_AUTO_JOIN_NETWORK".into());
        }
        Ok(())
    }
    fn local(&self, a: &Asset) -> Result<Vec<u8>, String> {
        self.0
            .lock()
            .unwrap()
            .local
            .get(&a.uuid)
            .cloned()
            .ok_or("SYNC_AUTO_JOIN_ASSET_MISSING".into())
    }
}
fn database() -> Database {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute("UPDATE sync_settings SET enabled=1,endpoint='https://isolated.invalid',bucket='fixture',region='us-east-1',object_key=?1,path_style=1", [MAIN]).unwrap();
    Database {
        connection: Mutex::new(c),
    }
}
fn empty(i: usize) -> Vec<u8> {
    let field = [
        "todos",
        "notes",
        "attachments",
        "rules",
        "links",
        "items",
        "definitions",
        "templates",
    ][i];
    let mut v = json!({"format_version":1});
    v[field] = json!([]);
    if i < 3 {
        v["device_id"] = ID.into();
        v["generated_at"] = 1.into();
    }
    if i == 0 {
        v["groups"] = json!([]);
    }
    serde_json::to_vec(&v).unwrap()
}
fn fixture() -> (Arc<Database>, PreparedManualSync, Claim, Cloud) {
    let db = Arc::new(database());
    let c = db.connection.lock().unwrap();
    let bucket = s3::Bucket::new(
        "fixture",
        s3::region::Region::Custom {
            region: "us-east-1".into(),
            endpoint: "https://isolated.invalid".into(),
        },
        s3::creds::Credentials::new(Some("fixture"), Some("fixture"), None, None, None).unwrap(),
    )
    .unwrap();
    let p = PreparedManualSync::from_test_space(&c, bucket, MAIN);
    let raw = empty(0);
    let file = FileEntry {
        name: "meta-0.json".into(),
        size: raw.len() as u64,
        sha256: space::hash(&raw),
        missing: false,
    };
    let plan = publication::Plan {
        version: 1,
        operation: ID.into(),
        cloud_operation: ID.into(),
        source: s3_sync::migration_source_binding(&c).unwrap(),
        cloud_hash: "a".repeat(64),
        main_key: MAIN.into(),
        confirmed: true,
        published: true,
        completed: vec![file.name.clone()],
        files: vec![file.clone()],
        objects: cloud::object_keys(MAIN)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(i, key)| cloud::ObjectEntry {
                key,
                etag: (i == 0).then(|| "\"source\"".into()),
                file: (i == 0).then(|| file.clone()),
            })
            .collect(),
    };
    let claim = Claim::from_plan(&plan).unwrap();
    let cloud = Cloud::default();
    {
        let mut cloud = cloud.0.lock().unwrap();
        cloud
            .objects
            .insert(space::claim_key(MAIN), claim.raw().unwrap());
        cloud
            .objects
            .insert(claim.ready_key().unwrap(), claim.raw().unwrap());
        for (i, key) in cloud::object_keys(&claim.main().unwrap())
            .unwrap()
            .iter()
            .enumerate()
        {
            cloud.objects.insert(
                key.clone(),
                crate::sync_space::encode(key, std::str::from_utf8(&empty(i)).unwrap())
                    .unwrap()
                    .into_bytes(),
            );
        }
        let target = p.retarget(&claim.main().unwrap(), p.epoch());
        let key = s3_sync::lifecycle_key(&target).unwrap();
        cloud.objects.insert(
            key.clone(),
            crate::sync_space::encode(&key, "{\"format_version\":1,\"terminals\":[]}")
                .unwrap()
                .into_bytes(),
        );
    }
    drop(c);
    (db, p, claim, cloud)
}
fn todo(c: &Connection, id: &str) {
    c.execute("INSERT INTO todos(uuid,title,created_at,updated_at,updated_by,sort_order) VALUES(?1,'offline',1,1,?2,0)", params![id, ID]).unwrap();
}
fn plan(c: &mut Connection, id: &str) {
    let expected = crate::daily_plan_store::list(c, DATE).unwrap().revision;
    crate::daily_plan_store::write(
        c,
        &crate::daily_plan_store::DailyPlanWrite {
            operation_uuid: uuid::Uuid::new_v4().to_string(),
            task_uuid: id.into(),
            plan_date: DATE.into(),
            action: "add".into(),
            expected,
        },
        5,
        ID,
    )
    .unwrap();
}
fn configured(db: &Database) -> String {
    db.connection
        .lock()
        .unwrap()
        .query_row("SELECT object_key FROM sync_settings", [], |r| r.get(0))
        .unwrap()
}
fn set_document(cloud: &Cloud, main: &str, index: usize, raw: &[u8]) {
    let key = cloud::object_keys(main).unwrap()[index].clone();
    let wire = crate::sync_space::encode(&key, std::str::from_utf8(raw).unwrap()).unwrap();
    cloud
        .0
        .lock()
        .unwrap()
        .objects
        .insert(key, wire.into_bytes());
}
fn add_asset(cloud: &Cloud, main: &str) {
    let db = database();
    let c = db.connection.lock().unwrap();
    c.execute("INSERT INTO notes(uuid,title,content,color,pinned,created_at,updated_at,updated_by) VALUES(?1,'note','body','default',0,1,1,?1)", [PEER]).unwrap();
    set_document(
        cloud,
        main,
        1,
        &serde_json::to_vec(&crate::note_sync::build_document(&c, 1).unwrap()).unwrap(),
    );
    let doc = json!({"format_version":1,"device_id":ID,"generated_at":1,"attachments":[{
        "uuid":ASSET,"note_uuid":PEER,"kind":"file","display_name":"test.txt","mime_type":"text/plain",
        "byte_size":4,"sha256":space::hash(b"data"),"sort_order":0,"created_at":1,"updated_at":1,"deleted_at":null,"updated_by":ID
    }]});
    set_document(cloud, main, 2, &serde_json::to_vec(&doc).unwrap());
    cloud.0.lock().unwrap().binaries.insert(
        (main.into(), ASSET.into(), "original".into()),
        b"data".to_vec(),
    );
}

#[test]
fn offline_task_and_plan_survive_and_old_epoch_cannot_ack() {
    tauri::async_runtime::block_on(async {
        let (db, p, claim, cloud) = fixture();
        {
            let mut c = db.connection.lock().unwrap();
            todo(&c, ID);
            plan(&mut c, ID);
        }
        let next = follow_with(&cloud, &db, &p).await.unwrap().unwrap();
        assert_eq!(configured(&db), claim.main().unwrap());
        let mut c = db.connection.lock().unwrap();
        assert!(space::is_active(&c).unwrap());
        assert!(p.require_current(&c).is_err());
        next.require_current(&c).unwrap();
        assert_eq!(
            crate::daily_plan_store::list(&mut c, DATE).unwrap().current[0].task_uuid,
            ID
        );
        let s = crate::daily_plan_store::snapshot(&mut c).unwrap();
        assert!(s.revision > s.synced_revision);
        assert!(s.etag.is_none());
        drop(c);
        let count = cloud.0.lock().unwrap().reads.len();
        assert!(follow_with(&cloud, &db, &next).await.unwrap().is_none());
        assert_eq!(cloud.0.lock().unwrap().reads.len(), count);
    });
}

#[test]
fn workflow_autojoin_preserves_local_source_target_and_invalidates_receipts() {
    tauri::async_runtime::block_on(async {
        let (db, p, claim, cloud) = fixture();
        let old_snapshot;
        {
            let mut c = db.connection.lock().unwrap();
            todo(&c, ID);
            let request = crate::task_workflow_store::WorkflowWrite {
                operation_uuid: uuid::Uuid::new_v4().to_string(),
                task_uuid: ID.into(),
                state: "waiting".into(),
                reason: "local private".into(),
                review_date: None,
                date: DATE.into(),
                remove_from_plan: false,
                expected: crate::task_workflow_store::list(&mut c, DATE)
                    .unwrap()
                    .revision,
                expected_plan: None,
            };
            crate::task_workflow_store::write(&mut c, &request, 10, ID).unwrap();
            old_snapshot =
                crate::task_workflow_sync::prepare(&mut c, p.epoch(), None, None).unwrap();
        }
        for (main, clock, reason) in [
            (MAIN.to_string(), 20, "source private"),
            (claim.main().unwrap(), 30, "target private"),
        ] {
            let d = json!({"format_version":1,"events":[],"states":[{"task_uuid":PEER,"state":"waiting","reason":reason,"review_date":null,"clock":clock,"writer":ID,"basis":""}]});
            cloud.0.lock().unwrap().objects.insert(
                crate::task_workflow_sync::object_key(&main, &[]).unwrap(),
                d.to_string().into_bytes(),
            );
        }
        let peer = database();
        {
            let c = peer.connection.lock().unwrap();
            todo(&c, PEER);
            set_document(
                &cloud,
                MAIN,
                0,
                &serde_json::to_vec(&crate::sync::build_document(&c, 1).unwrap()).unwrap(),
            );
        }
        let next = follow_with(&cloud, &db, &p).await.unwrap().unwrap();
        let mut c = db.connection.lock().unwrap();
        let s = crate::task_workflow_store::snapshot(&mut c).unwrap();
        assert_eq!(s.document.states.len(), 2);
        assert!(s
            .document
            .states
            .iter()
            .any(|s| s.reason == "target private"));
        assert_eq!(
            crate::task_workflow_store::list(&mut c, DATE)
                .unwrap()
                .entries
                .len(),
            2
        );
        assert!(s.revision > s.synced_revision);
        assert!(s.etag.is_none());
        assert!(s.generation > 0);
        assert!(
            !crate::task_workflow_sync::acknowledge(&mut c, &old_snapshot, Some("\"old\""))
                .unwrap()
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM task_workflow_operations", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        assert!(crate::task_workflow_sync::prepare(&mut c, next.epoch(), None, None).is_ok());
    });
}

#[test]
fn workflow_autojoin_source_disappearance_and_corruption_preserve_target() {
    tauri::async_runtime::block_on(async {
        for corrupt in [false, true] {
            let (db, p, _, cloud) = fixture();
            db.connection
                .lock()
                .unwrap()
                .execute("UPDATE task_workflow_sync_state SET etag='\"seen\"'", [])
                .unwrap();
            if corrupt {
                cloud.0.lock().unwrap().objects.insert(
                    crate::task_workflow_sync::object_key(MAIN, &[]).unwrap(),
                    b"{}".to_vec(),
                );
            }
            assert!(follow_with(&cloud, &db, &p).await.is_err());
            assert_eq!(configured(&db), MAIN);
            assert_eq!(
                crate::task_workflow_store::snapshot(&mut db.connection.lock().unwrap())
                    .unwrap()
                    .generation,
                0
            );
        }
    });
}
#[test]
fn source_peer_metadata_and_completed_plan_are_preserved() {
    tauri::async_runtime::block_on(async {
        let (db, p, _, cloud) = fixture();
        let peer = database();
        let mut c = peer.connection.lock().unwrap();
        todo(&c, PEER);
        plan(&mut c, PEER);
        c.execute(
            "UPDATE todos SET completed=1,completed_at=10,updated_at=10 WHERE uuid=?1",
            [PEER],
        )
        .unwrap();
        c.execute(
            "UPDATE todos SET title='after completion',updated_at=11 WHERE uuid=?1",
            [PEER],
        )
        .unwrap();
        set_document(
            &cloud,
            MAIN,
            0,
            &serde_json::to_vec(&crate::sync::build_document(&c, 11).unwrap()).unwrap(),
        );
        let d = crate::daily_plan_store::snapshot(&mut c).unwrap().document;
        cloud.0.lock().unwrap().objects.insert(
            crate::daily_plan_sync::object_key(MAIN, &[]).unwrap(),
            crate::daily_plan_protocol::encode(&d).unwrap().into_bytes(),
        );
        drop(c);
        follow_with(&cloud, &db, &p).await.unwrap();
        let mut c = db.connection.lock().unwrap();
        let plans = crate::daily_plan_store::list(&mut c, DATE).unwrap();
        assert_eq!(plans.current.len(), 1);
        assert_eq!(plans.current[0].status, "completed");
        assert_eq!(
            crate::daily_plan_store::snapshot(&mut c)
                .unwrap()
                .document
                .events,
            d.events
        );
    });
}
#[test]
fn source_and_target_terminals_win_over_offline_and_newer_stale_bodies() {
    tauri::async_runtime::block_on(async {
        for source_terminal in [false, true] {
            let (db, p, claim, cloud) = fixture();
            {
                let mut c = db.connection.lock().unwrap();
                todo(&c, ID);
                plan(&mut c, ID);
                c.execute(
                    "UPDATE todos SET updated_at=9007199254740991 WHERE uuid=?1",
                    [ID],
                )
                .unwrap();
            }
            let terminal_main = if source_terminal {
                MAIN.to_string()
            } else {
                claim.main().unwrap()
            };
            let key = s3_sync::lifecycle_key(&p.retarget(&terminal_main, p.epoch())).unwrap();
            let ledger = json!({"format_version":1,"terminals":[{"kind":"todo","uuid":ID,"operation_uuid":ASSET,"purged_at":20}]}).to_string();
            cloud.0.lock().unwrap().objects.insert(
                key.clone(),
                crate::sync_space::encode(&key, &ledger)
                    .unwrap()
                    .into_bytes(),
            );
            follow_with(&cloud, &db, &p).await.unwrap();
            let mut c = db.connection.lock().unwrap();
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM todos", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert!(crate::daily_plan_store::list(&mut c, DATE)
                .unwrap()
                .current
                .is_empty());
            assert_eq!(crate::purge::terminals(&c).unwrap().len(), 1);
        }
    });
}
#[test]
fn missing_equal_mismatched_and_lost_response_assets() {
    tauri::async_runtime::block_on(async {
        for mode in ["missing", "equal", "mismatch", "lost"] {
            let (db, p, claim, cloud) = fixture();
            add_asset(&cloud, MAIN);
            let main = claim.main().unwrap();
            if mode == "equal" || mode == "mismatch" {
                cloud.0.lock().unwrap().binaries.insert(
                    (main.clone(), ASSET.into(), "original".into()),
                    if mode == "equal" { b"data" } else { b"nope" }.to_vec(),
                );
            }
            if mode == "lost" {
                cloud.0.lock().unwrap().lose_upload = true;
            }
            let r = follow_with(&cloud, &db, &p).await;
            if mode == "mismatch" {
                assert!(matches!(r, Err(ref e) if e == "SYNC_AUTO_JOIN_ASSET_MISMATCH"));
                assert_eq!(configured(&db), MAIN);
                continue;
            }
            if mode == "lost" {
                assert!(r.is_err());
                assert_eq!(configured(&db), MAIN);
                follow_with(&cloud, &db, &p).await.unwrap();
            } else {
                r.unwrap();
            }
            let c = cloud.0.lock().unwrap();
            assert_eq!(c.downloads, usize::from(mode != "equal"));
            assert_eq!(c.uploads, usize::from(mode != "equal"));
            assert_eq!(
                db.connection
                    .lock()
                    .unwrap()
                    .query_row(
                        "SELECT remote_uploaded FROM note_attachments WHERE uuid=?1",
                        [ASSET],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
        }
    });
}
#[test]
fn no_association_is_noop_but_denied_invalid_not_ready_never_fall_back() {
    tauri::async_runtime::block_on(async {
        for mode in [
            "none",
            "denied",
            "invalid",
            "not-ready",
            "incomplete",
            "binding",
            "ready-conflict",
        ] {
            let (db, p, claim, cloud) = fixture();
            {
                let mut c = cloud.0.lock().unwrap();
                match mode {
                    "none" => {
                        c.objects.remove(&space::claim_key(MAIN));
                    }
                    "denied" => {
                        c.fail = Some(space::claim_key(MAIN));
                    }
                    "invalid" => {
                        c.objects.insert(space::claim_key(MAIN), b"{}".to_vec());
                    }
                    "not-ready" => {
                        c.objects.remove(&claim.ready_key().unwrap());
                    }
                    "incomplete" => {
                        c.objects.remove(&claim.main().unwrap());
                    }
                    "binding" => {
                        let mut plan = claim.plan().unwrap();
                        plan.source = "c".repeat(64);
                        c.objects.insert(
                            space::claim_key(MAIN),
                            Claim::from_plan(&plan).unwrap().raw().unwrap(),
                        );
                    }
                    "ready-conflict" => {
                        let mut plan = claim.plan().unwrap();
                        plan.operation = PEER.into();
                        c.objects.insert(
                            claim.ready_key().unwrap(),
                            Claim::from_plan(&plan).unwrap().raw().unwrap(),
                        );
                    }
                    _ => unreachable!(),
                }
            }
            let r = follow_with(&cloud, &db, &p).await;
            if mode == "none" {
                assert!(r.unwrap().is_none());
            } else {
                assert!(r.is_err(), "{mode}");
            }
            assert_eq!(configured(&db), MAIN);
            assert_eq!(cloud.0.lock().unwrap().uploads, 0);
        }
    });
}
#[test]
fn interrupted_join_pins_association_and_stale_target_cannot_commit() {
    tauri::async_runtime::block_on(async {
        let (db, p, claim, cloud) = fixture();
        cloud
            .0
            .lock()
            .unwrap()
            .objects
            .remove(&claim.ready_key().unwrap());
        assert!(follow_with(&cloud, &db, &p).await.is_err());
        cloud
            .0
            .lock()
            .unwrap()
            .objects
            .remove(&space::claim_key(MAIN));
        assert!(
            matches!(follow_with(&cloud, &db, &p).await, Err(ref e) if e == "SYNC_AUTO_JOIN_ASSOCIATION_CHANGED")
        );
        cloud
            .0
            .lock()
            .unwrap()
            .objects
            .insert(space::claim_key(MAIN), claim.raw().unwrap());
        cloud
            .0
            .lock()
            .unwrap()
            .objects
            .insert(claim.ready_key().unwrap(), claim.raw().unwrap());
        let peer = db.clone();
        cloud.0.lock().unwrap().on_read = Some(Box::new(move || {
            let c = peer.connection.lock().unwrap();
            crate::sync_target::invalidate(&c).unwrap();
            crate::sync_target::activate(&c).unwrap();
        }));
        assert!(
            matches!(follow_with(&cloud, &db, &p).await, Err(ref e) if e == "SYNC_AUTO_JOIN_TARGET_CHANGED")
        );
        assert_eq!(configured(&db), MAIN);
    });
}
#[test]
fn old_cleanup_evidence_is_not_rebound_and_new_target_starts_without_old_ack() {
    tauri::async_runtime::block_on(async {
        let (db, p, claim, cloud) = fixture();
        add_asset(&cloud, MAIN);
        {
            let c = db.connection.lock().unwrap();
            crate::purge_remote::bind_target(&c, p.epoch()).unwrap();
            let raw =
                cloud.0.lock().unwrap().objects[&cloud::object_keys(MAIN).unwrap()[2]].clone();
            let document: note_attachment_sync::NoteAttachmentSyncDocument =
                serde_json::from_slice(&raw).unwrap();
            crate::purge_remote::save(
                &c,
                &crate::purge_remote::Evidence::from_asset(
                    p.epoch(),
                    MAIN,
                    &document.attachments[0],
                ),
            )
            .unwrap();
            c.execute(
                "UPDATE daily_plan_sync_state SET etag='\"seen\"',revision=10,synced_revision=10",
                [],
            )
            .unwrap();
        }
        // Previously seen source planning must still be present, while target may be first-use.
        cloud.0.lock().unwrap().objects.insert(
            crate::daily_plan_sync::object_key(MAIN, &[]).unwrap(),
            b"{\"format_version\":1,\"events\":[],\"plans\":[],\"completions\":[]}".to_vec(),
        );
        let key = s3_sync::lifecycle_key(&p.retarget(&claim.main().unwrap(), p.epoch())).unwrap();
        let terminal = json!({"format_version":1,"terminals":[{"kind":"note","uuid":PEER,"operation_uuid":ID,"purged_at":20}]}).to_string();
        cloud.0.lock().unwrap().objects.insert(
            key.clone(),
            crate::sync_space::encode(&key, &terminal)
                .unwrap()
                .into_bytes(),
        );
        let next = follow_with(&cloud, &db, &p).await.unwrap().unwrap();
        let mut c = db.connection.lock().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM app_metadata WHERE key=?1",
                [format!("purge.remote.evidence.v1:{}:{ASSET}", next.epoch())],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(crate::daily_plan_sync::prepare(&mut c, next.epoch(), None, None).is_ok());
        assert!(c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
                [format!("purge.remote.evidence.v1:{}:{ASSET}", p.epoch())],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        assert_eq!(cloud.0.lock().unwrap().uploads, 0);
    });
}

#[test]
fn source_seen_without_etag_and_atomic_revision_failure_do_not_switch() {
    tauri::async_runtime::block_on(async {
        for seen in [true, false] {
            let (db, p, _, cloud) = fixture();
            {
                let mut c = db.connection.lock().unwrap();
                todo(&c, ID);
                plan(&mut c, ID);
                if seen {
                    c.execute(
                        "INSERT INTO app_metadata(key,value) VALUES(?1,'1')",
                        [format!("daily.plan.remote.seen.v1:{}", p.epoch())],
                    )
                    .unwrap();
                } else {
                    c.execute(
                        "UPDATE recurrence_sync_state SET revision=9007199254740991",
                        [],
                    )
                    .unwrap();
                }
            }
            assert!(follow_with(&cloud, &db, &p).await.is_err());
            assert_eq!(configured(&db), MAIN);
            let mut c = db.connection.lock().unwrap();
            p.require_current(&c).unwrap();
            assert!(!space::is_active(&c).unwrap());
            assert_eq!(
                crate::daily_plan_store::list(&mut c, DATE)
                    .unwrap()
                    .current
                    .len(),
                1
            );
        }
    });
}

#[test]
fn association_probe_is_read_only_and_checks_full_target_protocol() {
    tauri::async_runtime::block_on(async {
        let (db, p, claim, cloud) = fixture();
        let before = db
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM app_metadata", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        assert!(association(&cloud, &db, &p, false).await.unwrap().is_some());
        bundle(&cloud, &db, &p, &claim.main().unwrap(), true)
            .await
            .unwrap();
        assert_eq!(configured(&db), MAIN);
        assert_eq!(
            before,
            db.connection
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM app_metadata", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()
        );
        let key = cloud::object_keys(&claim.main().unwrap()).unwrap()[4].clone();
        cloud
            .0
            .lock()
            .unwrap()
            .objects
            .insert(key, b"{\"format_version\":1,\"links\":[]}".to_vec());
        assert!(bundle(&cloud, &db, &p, &claim.main().unwrap(), true)
            .await
            .is_err());
    });
}

#[test]
fn https_legacy_http_opt_in_difference_joins_with_local_proof_and_keeps_settings() {
    tauri::async_runtime::block_on(async {
        for local_opt_in in [false, true] {
            let (db, p, claim, cloud) = fixture();
            let mut historical = claim.plan().unwrap();
            {
                let c = db.connection.lock().unwrap();
                c.execute("UPDATE sync_settings SET allow_http=?1", [!local_opt_in])
                    .unwrap();
                historical.source = s3_sync::migration_source_binding(&c).unwrap();
                c.execute("UPDATE sync_settings SET allow_http=?1", [local_opt_in])
                    .unwrap();
            }
            let claim = Claim::from_plan(&historical).unwrap();
            cloud
                .0
                .lock()
                .unwrap()
                .objects
                .insert(space::claim_key(MAIN), claim.raw().unwrap());
            cloud
                .0
                .lock()
                .unwrap()
                .objects
                .insert(claim.ready_key().unwrap(), claim.raw().unwrap());
            let next = follow_with(&cloud, &db, &p).await.unwrap().unwrap();
            let c = db.connection.lock().unwrap();
            next.require_current(&c).unwrap();
            assert!(space::is_active(&c).unwrap());
            assert_eq!(
                c.query_row("SELECT allow_http FROM sync_settings", [], |r| r
                    .get::<_, bool>(0))
                    .unwrap(),
                local_opt_in
            );
            drop(c);
            assert_eq!(configured(&db), claim.main().unwrap());
        }
    });
}

#[test]
fn https_compatibility_never_relaxes_real_target_identity_or_http_policy() {
    let (db, _, _, _) = fixture();
    let c = db.connection.lock().unwrap();
    for change in [
        "endpoint='https://other.invalid'",
        "region='other-region'",
        "bucket='other-bucket'",
        "object_key='other/todos.json'",
        "path_style=0",
    ] {
        c.execute_batch("SAVEPOINT mismatch").unwrap();
        c.execute("UPDATE sync_settings SET allow_http=1", [])
            .unwrap();
        c.execute(&format!("UPDATE sync_settings SET {change}"), [])
            .unwrap();
        let source = s3_sync::migration_source_binding(&c).unwrap();
        c.execute_batch("ROLLBACK TO mismatch; RELEASE mismatch")
            .unwrap();
        assert!(!s3_sync::matches_auto_join_source_binding(&c, &source).unwrap());
    }
    c.execute(
        "UPDATE sync_settings SET endpoint='http://isolated.invalid',allow_http=1",
        [],
    )
    .unwrap();
    let denied = serde_json::to_vec(&serde_json::json!([
        "http://isolated.invalid",
        "us-east-1",
        "fixture",
        MAIN,
        true,
        false
    ]))
    .unwrap();
    assert!(!s3_sync::matches_auto_join_source_binding(&c, &space::hash(&denied)).unwrap());
}
