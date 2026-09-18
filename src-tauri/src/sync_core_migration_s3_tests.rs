//! LC2a-2b isolated multi-object migration evidence; no production migration commands.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAIN: &str = "account/todos.json";
const PREFIX: &str = "account/migration-prototype/v2/";
const RULE: &str = "123e4567-e89b-42d3-a456-426614174030";
const GROUP: &str = "123e4567-e89b-42d3-a456-426614174040";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

const SPACE: &str = "eggdone-spaces/v2/00000000-0000-4000-8000-000000000001/";
async fn space_roundtrip() {
    let client = Client::new(TODO, NOTE);
    let p = s3_sync::PreparedManualSync::from_test_space(
        &client.db.connection.lock().unwrap(),
        bucket(SECRET),
        &format!("{SPACE}todos.json"),
    );
    let todos = s3_sync::download_remote(&p).await.unwrap();
    assert!(matches!(
        s3_sync::upload_document(&p, todos.document.as_ref().unwrap(), &todos)
            .await
            .unwrap(),
        s3_sync::UploadOutcome::Success
    ));
    let notes = s3_sync::download_note_remote(&p).await.unwrap();
    assert!(matches!(
        s3_sync::upload_note_document(&p, notes.document.as_ref().unwrap(), &notes)
            .await
            .unwrap(),
        s3_sync::UploadOutcome::Success
    ));
    let assets = s3_sync::download_note_attachment_remote(&p).await.unwrap();
    assert!(matches!(
        s3_sync::upload_note_attachment_document(&p, assets.document.as_ref().unwrap(), &assets)
            .await
            .unwrap(),
        s3_sync::UploadOutcome::Success
    ));
    let rules = p.recurrence_transport().unwrap();
    let r = rules.download().await.unwrap();
    assert!(matches!(
        rules
            .upload(r.document.as_ref().unwrap(), &r)
            .await
            .unwrap(),
        crate::recurrence_transport::RuleUploadOutcome::Uploaded { .. }
    ));
    let links = p.link_transport().unwrap();
    let r = links.download().await.unwrap();
    assert!(matches!(
        links
            .upload(r.document.as_ref().unwrap(), &r)
            .await
            .unwrap(),
        crate::task_note_link_transport::LinkUploadOutcome::Uploaded { .. }
    ));
    for domain in [
        crate::task_checklist_sync::Domain::Items,
        crate::task_checklist_sync::Domain::Definitions,
    ] {
        let t = p.checklist_transport(domain).unwrap();
        let r = t.download().await.unwrap();
        assert!(matches!(
            t.upload(r.document.as_ref().unwrap(), &r).await.unwrap(),
            crate::task_checklist_transport::ChecklistUploadOutcome::Uploaded { .. }
        ));
    }
    let t = p.template_transport().unwrap();
    let r = t.download().await.unwrap();
    assert!(matches!(
        t.upload(r.document.as_ref().unwrap(), &r).await.unwrap(),
        crate::task_checklist_transport::ChecklistUploadOutcome::Uploaded { .. }
    ));
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -SpaceProtocolSessions"]
fn space_protocol_prepare() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        for (i, key) in keys().iter().enumerate() {
            let source = target.get_object(key).await.unwrap();
            let text = std::str::from_utf8(source.as_slice()).unwrap();
            let dest = format!("{SPACE}{}", crate::sync_space::FILES[i]);
            let wire = crate::sync_space::encode(&dest, text).unwrap();
            assert!(
                validate(i, &wire).is_err(),
                "legacy parser must reject new space"
            );
            assert_eq!(
                target
                    .put_object(&dest, wire.as_bytes())
                    .await
                    .unwrap()
                    .status_code(),
                200
            );
        }
        space_roundtrip().await;
        println!("SPACE_PROTOCOL_DESKTOP_PREPARE_OK: eight production transports write/read v2; legacy parsers reject envelopes");
    });
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -SpaceProtocolSessions"]
fn space_protocol_verify() {
    tauri::async_runtime::block_on(async {
        space_roundtrip().await;
        let target = bucket(SECRET);
        for (i, key) in keys().iter().enumerate() {
            let source = target.get_object(key).await.unwrap();
            let dest = format!("{SPACE}{}", crate::sync_space::FILES[i]);
            let raw = target.get_object(&dest).await.unwrap();
            let wire = std::str::from_utf8(raw.as_slice()).unwrap();
            assert!(validate(i, wire).is_err());
            let inner = crate::sync_space::decode(&dest, wire).unwrap();
            validate(i, &inner).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&inner).unwrap(),
                serde_json::from_slice::<Value>(source.as_slice()).unwrap()
            );
        }
        let client = Client::new(TODO, NOTE);
        let p = s3_sync::PreparedManualSync::from_test_space(
            &client.db.connection.lock().unwrap(),
            bucket(SECRET),
            &format!("{SPACE}todos.json"),
        );
        let t = p.recurrence_transport().unwrap();
        let r = t.download().await.unwrap();
        let key = format!("{SPACE}recurrence-rules.json");
        let original = target.get_object(&key).await.unwrap();
        target
            .put_object(&key, b"{\"format_version\":1,\"rules\":[]}")
            .await
            .unwrap();
        assert!(t.download().await.is_err());
        assert!(matches!(
            t.upload(r.document.as_ref().unwrap(), &r).await.unwrap(),
            crate::recurrence_transport::RuleUploadOutcome::Conflict
        ));
        target.delete_object(&key).await.unwrap();
        assert!(matches!(t.download().await, Err(e) if e == "SYNC_SPACE_INCOMPLETE"));
        target.put_object(&key, original.as_slice()).await.unwrap();
        let main = format!("{SPACE}todos.json");
        let original = target.get_object(&main).await.unwrap();
        target.delete_object(&main).await.unwrap();
        assert!(
            matches!(s3_sync::download_remote(&p).await, Err(e) if e == "SYNC_SPACE_INCOMPLETE")
        );
        target.put_object(&main, original.as_slice()).await.unwrap();
        println!("SPACE_PROTOCOL_DESKTOP_VERIFY_OK: Harmony roundtrip, unchanged legacy data, mixed format/missing objects refused, stale ETag cannot overwrite");
    });
}

fn keys() -> Vec<String> {
    vec![
        MAIN.into(),
        s3_sync::derive_note_object_key(MAIN),
        s3_sync::derive_note_attachment_object_key(MAIN),
        crate::recurrence_protocol::recurrence_object_key(MAIN, &[]).unwrap(),
        crate::task_note_link_protocol::object_key(MAIN, &[]).unwrap(),
        crate::task_checklist_sync::Domain::Items
            .object_key(MAIN, &[])
            .unwrap(),
        crate::task_checklist_sync::Domain::Definitions
            .object_key(MAIN, &[])
            .unwrap(),
        crate::task_template_sync::object_key(MAIN, &[]).unwrap(),
    ]
}

fn validate(index: usize, text: &str) -> Result<(), String> {
    let error = |e: serde_json::Error| e.to_string();
    match index {
        0 => crate::sync::validate_document(&serde_json::from_str(text).map_err(error)?),
        1 => crate::note_sync::validate_document(&serde_json::from_str(text).map_err(error)?),
        2 => crate::note_attachment_sync::validate_document(
            &serde_json::from_str(text).map_err(error)?,
        ),
        3 => crate::recurrence_protocol::parse_document(text).map(|_| ()),
        4 => crate::task_note_link_protocol::parse_document(text).map(|_| ()),
        5 => crate::task_checklist_protocol::parse_items(text).map(|_| ()),
        6 => crate::task_checklist_protocol::parse_definitions(text).map(|_| ()),
        7 => crate::task_template_protocol::parse(text).map(|_| ()),
        _ => Err("Unknown migration domain".into()),
    }
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -CloudSnapshotSessions"]
fn cloud_snapshot_verify() {
    use crate::migration_backup::{self as backup, cloud};
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let source = s3_sync::MigrationAssetSource::from_test_bucket(target.clone(), MAIN);
        let remote = source.metadata().await.unwrap();
        assert_eq!(remote.len(), 8);
        assert!(remote.iter().all(|r| r.etag.is_some() && r.bytes.is_some()));
        let root =
            std::env::temp_dir().join(format!("eggdone-native-cloud-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut c = Connection::open(root.join("fixture.sqlite")).unwrap();
        crate::db::migrate(&mut c).unwrap();
        c.execute("UPDATE sync_settings SET endpoint='https://isolated.invalid',region='us-east-1',bucket='fixture',object_key=?1", [MAIN]).unwrap();
        let binding = s3_sync::migration_source_binding(&c).unwrap();
        let work = backup::prepare(&mut c, 1).unwrap();
        let copies = root.join("backups");
        backup::copy(&work, &copies, &root.join("assets")).unwrap();
        backup::finish(&mut c, &work.plan, 2).unwrap();
        let local = backup::latest(&c).unwrap().unwrap();
        let plan = cloud::prepare(&mut c, &local, &binding, MAIN, &remote, 3).unwrap();
        assert_eq!(plan.assets.len(), 1);
        let runtime = s3_sync::SyncRuntime::default();
        let mut assets = std::collections::BTreeMap::new();
        for f in &plan.assets {
            assets.insert(
                f.name.clone(),
                source
                    .download(
                        &runtime,
                        &f.name[..36],
                        &f.name[37..],
                        f.size as i64,
                        &f.sha256,
                    )
                    .await
                    .unwrap(),
            );
        }
        cloud::copy(&copies, &local, &plan, &remote, |f| {
            Ok(assets[&f.name].clone())
        })
        .unwrap();
        cloud::require_remote(&plan, &source.metadata().await.unwrap()).unwrap();
        cloud::finish(&mut c, &local, &plan, 4).unwrap();
        drop(c);
        let c = Connection::open(root.join("fixture.sqlite")).unwrap();
        let saved = cloud::latest(&c).unwrap().unwrap();
        cloud::verify_files(&copies, &local, &saved).unwrap();
        for (i, key) in keys().iter().enumerate() {
            let raw = target.get_object(key).await.unwrap();
            assert_eq!(
                raw.as_slice(),
                remote[i].bytes.as_ref().unwrap(),
                "snapshot must not change source"
            );
        }
        drop(c);
        std::fs::remove_dir_all(&root).unwrap();
        println!("CLOUD_SNAPSHOT_DESKTOP_OK: native eight-domain GET, raw bytes, remote-only asset and durable copy verified");
    });
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -PublicationSessions"]
fn publication_verify() {
    use crate::migration_backup::{self as backup, cloud, publication as p};
    use std::cell::RefCell;
    let target = bucket(SECRET);
    let source = s3_sync::MigrationAssetSource::from_test_bucket(target.clone(), MAIN);
    let remote = tauri::async_runtime::block_on(source.metadata()).unwrap();
    let root =
        std::env::temp_dir().join(format!("eggdone-publication-s3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let mut c = Connection::open(root.join("db")).unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute_batch("UPDATE sync_settings SET endpoint='https://fixture.invalid',region='us-east-1',bucket='fixture',object_key='account/todos.json';
      INSERT INTO app_metadata(key,value) VALUES('sync.target.epoch.v1','settled');
      UPDATE sync_runtime_state SET last_result='success',last_success_at=1,dirty_domains='[]';
      UPDATE recurrence_sync_state SET synced_revision=revision;UPDATE task_note_link_sync_state SET synced_revision=revision;
      UPDATE task_checklist_sync_state SET synced_revision=revision;UPDATE task_template_sync_state SET synced_revision=revision;").unwrap();
    let work = backup::prepare(&mut c, 1).unwrap();
    let copies = root.join("copies");
    backup::copy(&work, &copies, &root.join("assets")).unwrap();
    backup::finish(&mut c, &work.plan, 2).unwrap();
    let local = backup::latest(&c).unwrap().unwrap();
    let binding = s3_sync::migration_source_binding(&c).unwrap();
    let cloud = cloud::prepare(&mut c, &local, &binding, MAIN, &remote, 3).unwrap();
    let runtime = s3_sync::SyncRuntime::default();
    cloud::copy(&copies, &local, &cloud, &remote, |f| {
        tauri::async_runtime::block_on(source.download(
            &runtime,
            &f.name[..36],
            &f.name[37..],
            f.size as i64,
            &f.sha256,
        ))
    })
    .unwrap();
    cloud::finish(&mut c, &local, &cloud, 4).unwrap();
    let cloud = cloud::latest(&c).unwrap().unwrap();
    let plan = p::prepare(&mut c, &local, &cloud).unwrap();
    let hash = p::plan_digest(&plan).unwrap();
    p::record(&mut c, &local, &cloud, &plan, &hash, None, false).unwrap();
    let plan = p::latest(&c).unwrap().unwrap();
    let c = RefCell::new(c);
    let mut io = s3_sync::MigrationStagingTarget::new(&source, &plan.operation).unwrap();
    for _ in 0..2 {
        p::publish(
            &plan,
            &mut io,
            |f| cloud::read_file(&copies, &local, &cloud, f),
            || p::require_current(&c.borrow(), &local, &cloud, &plan),
            || cloud::require_remote(&cloud, &tauri::async_runtime::block_on(source.metadata())?),
            |done, published| {
                p::record(
                    &mut c.borrow_mut(),
                    &local,
                    &cloud,
                    &plan,
                    &hash,
                    done,
                    published,
                )
            },
        )
        .unwrap();
    }
    let saved = p::latest(&c.borrow()).unwrap().unwrap();
    assert!(saved.published);
    let wire = serde_json::to_vec(&saved).unwrap();
    assert_eq!(
        tauri::async_runtime::block_on(target.put_object("account/publication-proof.json", &wire))
            .unwrap()
            .status_code(),
        200
    );
    // Real conditional create must refuse different contents, never overwrite an existing seed.
    let mut conflict = target.clone();
    conflict.extra_headers.insert(
        http::HeaderName::from_static("if-none-match"),
        http::HeaderValue::from_static("*"),
    );
    assert_eq!(
        tauri::async_runtime::block_on(
            conflict.put_object(&p::marker_key(&plan).unwrap(), b"other")
        )
        .unwrap()
        .status_code(),
        412
    );
    assert_eq!(
        tauri::async_runtime::block_on(target.get_object(&p::marker_key(&plan).unwrap()))
            .unwrap()
            .as_slice(),
        p::manifest(&plan).unwrap()
    );
    cloud::require_remote(
        &cloud,
        &tauri::async_runtime::block_on(source.metadata()).unwrap(),
    )
    .unwrap();
    drop(c);
    std::fs::remove_dir_all(root).unwrap();
    println!("PUBLICATION_DESKTOP_OK: native durable seed, If-None-Match, readback, idempotent retry and unchanged old space");
}

fn fixture() -> Vec<Value> {
    let client = Client::new(TODO, NOTE);
    client.add_file();
    let db = client.db.connection.lock().unwrap();
    let mut main = serde_json::to_value(crate::sync::build_document(&db, 1000).unwrap()).unwrap();
    main["groups"] = json!([{"uuid":GROUP,"name":"Migration fixture","color":"#f8c555",
        "sort_order":0,"created_at":100,"updated_at":100,"deleted_at":null,"updated_by":main["device_id"]}]);
    main["todos"][0]["group_uuid"] = json!(GROUP);
    let notes = serde_json::to_value(crate::note_sync::build_document(&db, 1000).unwrap()).unwrap();
    let attachments = serde_json::to_value(
        crate::note_attachment_sync::build_backup_document(&db, 1000).unwrap(),
    )
    .unwrap();
    let links =
        serde_json::to_value(crate::task_note_link_store::snapshot(&db).unwrap().document).unwrap();
    let recurrence: Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule = recurrence["base_rule"].clone();
    rule["uuid"] = json!(RULE);
    rule["first_todo_uuid"] = json!(TODO);
    rule["current_todo_uuid"] = json!(TODO);
    // Leave the fixture's deliberately invalid UTF-16 cases unparsed.
    let checklist: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(include_str!("../../docs/fixtures/task-checklist-v1.json")).unwrap();
    let mut item: Value = serde_json::from_str(checklist["base_item"].get()).unwrap();
    item["todo_uuid"] = json!(TODO);
    item["completed"] = json!(true);
    let mut definition: Value = serde_json::from_str(checklist["base_definition"].get()).unwrap();
    definition["rule_uuid"] = json!(RULE);
    definition["first_todo_uuid"] = json!(TODO);
    let templates: Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/task-templates-v1-canonical.json"
    ))
    .unwrap();
    vec![
        main,
        notes,
        attachments,
        json!({"format_version":1,"rules":[rule]}),
        links,
        json!({"format_version":1,"items":[item]}),
        json!({"format_version":1,"definitions":[definition]}),
        serde_json::from_str(templates["base"].as_str().unwrap()).unwrap(),
    ]
}

#[test]
fn migration_domains_use_production_validators() {
    let docs = fixture();
    for (i, doc) in docs.iter().enumerate() {
        validate(i, &doc.to_string()).unwrap();
        let mut invalid = doc.clone();
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
        invalid[field][0][if i == 6 { "rule_uuid" } else { "uuid" }] = json!("invalid-identity");
        assert!(
            validate(i, &invalid.to_string()).is_err(),
            "domain {i} invalid identity accepted"
        );
    }
    assert_eq!(keys().len(), 8);
}

#[test]
#[ignore = "Use run-sync-core-s3.ps1 -MigrationJournalSessions"]
fn migration_prepare() {
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
        // Real production builders/fixtures and parsers, not a new app sync implementation.
        for (i, (key, document)) in keys().iter().zip(fixture()).enumerate() {
            let bytes = serde_json::to_vec_pretty(&document).unwrap();
            validate(i, std::str::from_utf8(&bytes).unwrap()).unwrap();
            assert_eq!(
                target.put_object(key, &bytes).await.unwrap().status_code(),
                200
            );
        }
        let asset = format!(
            "{}{ASSET}/original",
            s3_sync::derive_note_asset_prefix(MAIN)
        );
        assert_eq!(
            target
                .put_object(&asset, BYTES)
                .await
                .unwrap()
                .status_code(),
            200
        );
        println!("MIGRATION_DESKTOP_PREPARE_OK: eight validated domains and one binary fixture");
    });
}

#[test]
#[ignore = "Requires the Harmony multi-object migration publication"]
fn migration_verify() {
    tauri::async_runtime::block_on(async {
        let target = bucket(SECRET);
        let published = target
            .get_object(&format!("{PREFIX}migration.json"))
            .await
            .unwrap();
        assert_eq!(published.status_code(), 200);
        let manifest: Value = serde_json::from_slice(published.as_slice()).unwrap();
        assert_eq!(manifest["format"], "eggdone.migration.fixture.v2");
        let plan = &manifest["plan"];
        let entries = plan["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 9);
        let space = plan["space"].as_str().unwrap();
        uuid::Uuid::parse_str(space).unwrap();
        let metadata = keys();
        for (i, entry) in entries.iter().enumerate() {
            let source = entry["key"].as_str().unwrap();
            if i < 8 {
                assert_eq!(source, metadata[i]);
            } else {
                assert_eq!(
                    source,
                    format!(
                        "{}{ASSET}/original",
                        s3_sync::derive_note_asset_prefix(MAIN)
                    )
                );
            }
            let sha = entry["sha256"].as_str().unwrap();
            assert_eq!(sha.len(), 64);
            let key = format!(
                "{PREFIX}{space}/objects/{}/{sha}",
                digest(source.as_bytes())
            );
            let copied = target.get_object(&key).await.unwrap();
            let original = target.get_object(source).await.unwrap();
            assert_eq!(copied.status_code(), 200);
            assert_eq!(original.status_code(), 200);
            assert_eq!(copied.as_slice(), original.as_slice());
            assert_eq!(digest(copied.as_slice()), sha);
            assert_eq!(
                copied.as_slice().len() as u64,
                entry["size"].as_u64().unwrap()
            );
            if i < 8 {
                validate(i, std::str::from_utf8(copied.as_slice()).unwrap()).unwrap();
            } else {
                assert_eq!(copied.as_slice(), BYTES);
            }
        }
        // The old client writes the old object only after byte-for-byte proof above.
        let old = target.get_object(MAIN).await.unwrap();
        let mut late: Value = serde_json::from_slice(old.as_slice()).unwrap();
        late["todos"][0]["title"] = json!("Late write remains in old space");
        late["todos"][0]["updated_at"] = json!(now_millis());
        validate(0, &late.to_string()).unwrap();
        assert_eq!(
            target
                .put_object(MAIN, &serde_json::to_vec(&late).unwrap())
                .await
                .unwrap()
                .status_code(),
            200
        );
        assert_eq!(
            target
                .get_object(&format!("{PREFIX}migration.json"))
                .await
                .unwrap()
                .as_slice(),
            published.as_slice()
        );
        println!("MIGRATION_DESKTOP_VERIFY_OK: production parsers accept copies; old-space write stays isolated");
    });
}
