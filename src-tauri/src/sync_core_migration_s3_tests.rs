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
