//! Explicit, immutable source association and transactional admission to terminal-aware spaces.
use crate::{
    lifecycle_sync,
    migration_backup::{self as backup, publication as seed, FileEntry},
    sync_space,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const PROOF: &str = "sync.space.activation.v1";
pub const PENDING: &str = "migration.backup.space.v1";
pub const LIMIT: usize = 20 * 1024 * 1024;
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid() -> String {
    "MIGRATION_SPACE_INVALID".into()
}
fn db(_: rusqlite::Error) -> String {
    "MIGRATION_SPACE_DATABASE".into()
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Claim(pub u32, pub String, pub String);
impl Claim {
    pub fn from_plan(p: &seed::Plan) -> Result<Self, String> {
        seed::validate(p)?;
        Ok(Self(
            1,
            p.main_key.clone(),
            String::from_utf8(seed::manifest(p)?).map_err(|_| invalid())?,
        ))
    }
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > 16 * 1024 * 1024 {
            return Err(invalid());
        }
        let c: Self = serde_json::from_slice(raw).map_err(|_| invalid())?;
        c.plan()?;
        Ok(c)
    }
    pub fn raw(&self) -> Result<Vec<u8>, String> {
        self.plan()?;
        serde_json::to_vec(self).map_err(|_| invalid())
    }
    pub fn plan(&self) -> Result<seed::Plan, String> {
        if self.0 != 1 || sync_space::scope(&self.1)?.is_some() {
            return Err(invalid());
        }
        let m: (
            String,
            String,
            String,
            String,
            Vec<(
                String,
                Option<String>,
                Option<String>,
                Option<u64>,
                Option<String>,
            )>,
            Vec<(String, u64, String)>,
        ) = serde_json::from_str(&self.2).map_err(|_| invalid())?;
        if m.0 != seed::FORMAT {
            return Err(invalid());
        }
        let mut objects = Vec::new();
        for (key, etag, name, size, sha) in m.4 {
            let file = match (name, size, sha) {
                (Some(name), Some(size), Some(sha256)) => Some(FileEntry { name, size, sha256 }),
                (None, None, None) => None,
                _ => return Err(invalid()),
            };
            objects.push(backup::cloud::ObjectEntry { key, etag, file });
        }
        let files: Vec<_> =
            m.5.into_iter()
                .map(|(name, size, sha256)| FileEntry { name, size, sha256 })
                .collect();
        let p = seed::Plan {
            version: 1,
            operation: m.1.clone(),
            cloud_operation: m.1,
            source: m.2,
            cloud_hash: m.3,
            main_key: self.1.clone(),
            completed: files.iter().map(|f| f.name.clone()).collect(),
            files,
            objects,
            confirmed: true,
            published: true,
        };
        seed::validate(&p)?;
        Ok(p)
    }
    pub fn main(&self) -> Result<String, String> {
        Ok(format!(
            "{}{}{}",
            sync_space::PREFIX,
            self.plan()?.operation,
            "/todos.json"
        ))
    }
    pub fn ready_key(&self) -> Result<String, String> {
        Ok(self.main()?.replace("todos.json", "ready.json"))
    }
}
pub fn claim_key(main: &str) -> String {
    format!("eggdone-space-links/v2/{}.json", hash(main.as_bytes()))
}
pub trait Storage {
    fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String>;
    fn create(&mut self, key: &str, bytes: &[u8], mime: &str) -> Result<(), String>;
}
pub fn association(storage: &mut impl Storage, main: &str) -> Result<Option<Claim>, String> {
    let c = storage
        .read(&claim_key(main), 16 * 1024 * 1024)?
        .map(|b| Claim::parse(&b))
        .transpose()?;
    if c.as_ref().is_some_and(|c| c.1 != main) {
        return Err(invalid());
    }
    Ok(c)
}
pub fn create_exact(
    storage: &mut impl Storage,
    key: &str,
    bytes: &[u8],
    mime: &str,
) -> Result<(), String> {
    storage.create(key, bytes, mime)?;
    if storage.read(key, LIMIT)?.as_deref() != Some(bytes) {
        return Err("MIGRATION_SPACE_CONFLICT".into());
    }
    Ok(())
}
fn empty(index: usize, id: &str) -> Result<String, String> {
    let mut v = json!({"format_version":1});
    if index < 3 {
        v["device_id"] = id.into();
        v["generated_at"] = 0.into();
    }
    let field = [
        "todos",
        "notes",
        "attachments",
        "rules",
        "links",
        "items",
        "definitions",
        "templates",
    ][index];
    v[field] = json!([]);
    if index == 0 {
        v["groups"] = json!([]);
    }
    let raw = v.to_string();
    backup::cloud::validate_document(index, raw.as_bytes())?;
    Ok(raw)
}
// Only the immutable seed is cross-checked: live S3 domains are not an atomic snapshot.
fn validate_seed_references(documents: &[Value]) -> Result<(), String> {
    let todos: std::collections::BTreeSet<_> = documents[0]["todos"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .filter_map(|v| v["uuid"].as_str())
        .collect();
    let notes: std::collections::BTreeSet<_> = documents[1]["notes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .filter_map(|v| v["uuid"].as_str())
        .collect();
    for (index, field, parent, ids) in [
        (2, "attachments", "note_uuid", &notes),
        (4, "links", "todo_uuid", &todos),
        (4, "links", "note_uuid", &notes),
        (5, "items", "todo_uuid", &todos),
        (3, "rules", "current_todo_uuid", &todos),
    ] {
        for row in documents[index][field].as_array().ok_or_else(invalid)? {
            if !row["deleted_at"].is_null() || (index == 3 && row["exhausted"] == true) {
                continue;
            }
            if !row[parent].as_str().is_some_and(|id| ids.contains(id)) {
                return Err("MIGRATION_SPACE_REFERENCES".into());
            }
        }
    }
    Ok(())
}
pub fn initialize(
    storage: &mut impl Storage,
    claim: &Claim,
    mut guard: impl FnMut() -> Result<(), String>,
) -> Result<lifecycle_sync::Document, String> {
    let p = claim.plan()?;
    let raw = claim.raw()?;
    guard()?;
    if let Some(existing) = association(storage, &claim.1)? {
        if existing != *claim {
            return Err("MIGRATION_SPACE_CHANGED".into());
        }
        if let Some(ready) = storage.read(&claim.ready_key()?, 16 * 1024 * 1024)? {
            if Claim::parse(&ready)? != *claim {
                return Err("MIGRATION_SPACE_CONFLICT".into());
            }
            return verify_current(storage, claim);
        }
    }
    let manifest = storage
        .read(&seed::marker_key(&p)?, 16 * 1024 * 1024)?
        .ok_or("MIGRATION_SPACE_SEED_MISSING")?;
    let decoded: Value = serde_json::from_slice(&manifest).map_err(|_| invalid())?;
    if decoded != serde_json::from_str::<Value>(&claim.2).map_err(|_| invalid())? {
        return Err("MIGRATION_SPACE_CONFLICT".into());
    }
    let prefix = claim.main()?.trim_end_matches("todos.json").to_string();
    let mut documents = Vec::new();
    let mut wires = Vec::new();
    let mut attachments = None;
    for (i, o) in p.objects.iter().enumerate() {
        guard()?;
        let body = match &o.file {
            Some(f) => read_seed(storage, &p, f)?,
            None => empty(i, &p.operation)?.into_bytes(),
        };
        backup::cloud::validate_document(i, &body)?;
        if i == 2 {
            attachments = Some(
                serde_json::from_slice::<crate::note_attachment_sync::NoteAttachmentSyncDocument>(
                    &body,
                )
                .map_err(|_| invalid())?,
            );
        }
        documents.push(serde_json::from_slice(&body).map_err(|_| invalid())?);
        let key = format!("{prefix}{}", sync_space::FILES[i]);
        wires.push(sync_space::encode(
            &key,
            std::str::from_utf8(&body).map_err(|_| invalid())?,
        )?);
    }
    validate_seed_references(&documents)?;
    let attachments = attachments.ok_or_else(invalid)?;
    let mut expected = std::collections::BTreeSet::new();
    let mut binaries = Vec::new();
    for a in &attachments.attachments {
        for (suffix, size, sha, mime) in [
            (
                "original",
                a.byte_size,
                Some(a.sha256.as_str()),
                a.mime_type.as_str(),
            ),
            (
                "preview.jpg",
                a.preview_byte_size.unwrap_or(0),
                a.preview_sha256.as_deref(),
                "image/jpeg",
            ),
        ] {
            if suffix == "preview.jpg" && a.kind != "image" {
                continue;
            }
            let f = FileEntry {
                name: format!("{}-{suffix}", a.uuid),
                size: size as u64,
                sha256: sha.ok_or_else(invalid)?.into(),
            };
            if !p.files.contains(&f) {
                return Err(invalid());
            }
            expected.insert(f.name.clone());
            binaries.push((
                format!("{prefix}note-assets/v1/{}/{suffix}", a.uuid),
                f,
                mime.to_owned(),
            ));
        }
    }
    if p.files
        .iter()
        .any(|f| !f.name.starts_with("meta-") && !expected.contains(&f.name))
    {
        return Err(invalid());
    }
    guard()?;
    storage.create(&claim_key(&claim.1), &raw, "application/json")?;
    if association(storage, &claim.1)?.as_ref() != Some(claim) {
        return Err("MIGRATION_SPACE_CHANGED".into());
    }
    for (i, wire) in wires.iter().enumerate() {
        guard()?;
        create_exact(
            storage,
            &format!("{prefix}{}", sync_space::FILES[i]),
            wire.as_bytes(),
            "application/json",
        )?;
    }
    for (key, file, mime) in binaries {
        guard()?;
        let bytes = read_seed(storage, &p, &file)?;
        create_exact(storage, &key, &bytes, &mime)?;
    }
    let key = format!("{prefix}lifecycle-terminals.json");
    let body = sync_space::encode(&key, "{\"format_version\":1,\"terminals\":[]}")?;
    create_exact(storage, &key, body.as_bytes(), "application/json")?;
    guard()?;
    create_exact(storage, &claim.ready_key()?, &raw, "application/json")?;
    verify_current(storage, claim)
}
fn read_seed(storage: &mut impl Storage, p: &seed::Plan, f: &FileEntry) -> Result<Vec<u8>, String> {
    let b = storage
        .read(&seed::object_key(p, f)?, f.size as usize)?
        .ok_or("MIGRATION_SPACE_SEED_MISSING")?;
    if b.len() as u64 != f.size || hash(&b) != f.sha256 {
        return Err("MIGRATION_SPACE_CONFLICT".into());
    }
    Ok(b)
}
pub fn verify_current(
    storage: &mut impl Storage,
    claim: &Claim,
) -> Result<lifecycle_sync::Document, String> {
    let prefix = claim.main()?.trim_end_matches("todos.json").to_string();
    for (i, file) in sync_space::FILES.iter().enumerate() {
        let key = format!("{prefix}{file}");
        let bytes = storage
            .read(&key, 16 * 1024 * 1024)?
            .ok_or("SYNC_SPACE_INCOMPLETE")?;
        let text = sync_space::decode(&key, std::str::from_utf8(&bytes).map_err(|_| invalid())?)?;
        backup::cloud::validate_document(i, text.as_bytes())?;
    }
    let key = format!("{prefix}lifecycle-terminals.json");
    let bytes = storage
        .read(&key, 4 * 1024 * 1024)?
        .ok_or("SYNC_SPACE_INCOMPLETE")?;
    lifecycle_sync::parse(&sync_space::decode(
        &key,
        std::str::from_utf8(&bytes).map_err(|_| invalid())?,
    )?)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    version: u32,
    main: String,
    binding: String,
    claim: Claim,
}
pub fn is_active(c: &Connection) -> Result<bool, String> {
    let raw: Option<String> = c
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [PROOF],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    let Some(raw) = raw else { return Ok(false) };
    let p: Proof = serde_json::from_str(&raw).map_err(|_| invalid())?;
    let main: String = c
        .query_row("SELECT object_key FROM sync_settings WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(db)?;
    Ok(p.version == 1
        && p.main == main
        && p.claim.main()? == main
        && p.binding == crate::s3_sync::migration_source_binding(c)?)
}
pub fn admit(c: &Connection, key: &str) -> Result<(), String> {
    if sync_space::scope(key)?.is_some() && !is_active(c)? {
        return Err("SYNC_SPACE_ACTIVATION_REQUIRED".into());
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub claim: Claim,
    pub local: backup::BackupPlan,
    pub mode: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub state: String,
    pub mode: String,
    pub confirmation: Option<String>,
    pub object_key: Option<String>,
}
impl Pending {
    pub fn token(&self) -> Result<String, String> {
        Ok(hash(&serde_json::to_vec(self).map_err(|_| invalid())?))
    }
    pub fn report(&self) -> Result<Report, String> {
        Ok(Report {
            state: "prepared".into(),
            mode: self.mode.clone(),
            confirmation: Some(self.token()?),
            object_key: Some(self.claim.main()?),
        })
    }
}
pub fn pending(c: &Connection) -> Result<Option<Pending>, String> {
    let raw: Option<String> = c
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [PENDING],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    raw.map(|r| {
        let p: Pending = serde_json::from_str(&r).map_err(|_| invalid())?;
        p.claim.plan()?;
        if !["create", "join"].contains(&p.mode.as_str()) || p.local.verified_at.is_none() {
            return Err(invalid());
        }
        Ok(p)
    })
    .transpose()
}
pub fn save_pending(c: &Connection, p: &Pending) -> Result<(), String> {
    c.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![PENDING,serde_json::to_string(p).map_err(|_|invalid())?]).map_err(db)?;
    Ok(())
}
pub fn activate(
    c: &mut Connection,
    p: &Pending,
    ledger: &lifecycle_sync::Document,
) -> Result<(), String> {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db)?;
    backup::require_current(&tx, &p.local)?;
    if p.local.verified_at.is_none() {
        return Err("MIGRATION_CLOUD_LOCAL_REQUIRED".into());
    }
    if crate::s3_sync::migration_source_binding(&tx)? != p.claim.plan()?.source {
        return Err("MIGRATION_SPACE_CHANGED".into());
    }
    crate::sync_target::invalidate_in_transaction(&tx)?;
    tx.execute(
        "UPDATE sync_settings SET object_key=?1 WHERE id=1",
        [p.claim.main()?],
    )
    .map_err(db)?;
    crate::sync_target::activate(&tx)?;
    let epoch = crate::sync_target::capture(&tx)?;
    let proof = Proof {
        version: 1,
        main: p.claim.main()?,
        binding: crate::s3_sync::migration_source_binding(&tx)?,
        claim: p.claim.clone(),
    };
    tx.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![PROOF,serde_json::to_string(&proof).map_err(|_|invalid())?]).map_err(db)?;
    // Surviving local binaries were copied and verified before committing the switch.
    tx.execute("UPDATE note_attachments SET remote_uploaded=1", [])
        .map_err(db)?;
    lifecycle_sync::prepare_in_transaction(&tx, &epoch, ledger)?;
    tx.execute("DELETE FROM app_metadata WHERE key=?1", [PENDING])
        .map_err(db)?;
    tx.commit().map_err(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    #[derive(Default, Clone)]
    struct Cloud {
        objects: BTreeMap<String, Vec<u8>>,
        calls: usize,
        fail: usize,
        after: bool,
    }
    impl Cloud {
        fn event(&mut self, after: bool) -> Result<(), String> {
            if !after {
                self.calls += 1;
            }
            if self.calls == self.fail && self.after == after {
                return Err("MIGRATION_PUBLICATION_NETWORK".into());
            }
            Ok(())
        }
    }
    impl Storage for Cloud {
        fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
            self.event(false)?;
            let value = self.objects.get(key).cloned();
            assert!(value.as_ref().is_none_or(|v| v.len() <= limit));
            self.event(true)?;
            Ok(value)
        }
        fn create(&mut self, key: &str, bytes: &[u8], _: &str) -> Result<(), String> {
            self.event(false)?;
            self.objects
                .entry(key.into())
                .or_insert_with(|| bytes.to_vec());
            self.event(true)
        }
    }
    fn seed_fixture() -> (Claim, Cloud) {
        let bytes = empty(0, ID).unwrap().into_bytes();
        let f = FileEntry {
            name: "meta-0.json".into(),
            size: bytes.len() as u64,
            sha256: hash(&bytes),
        };
        let p = seed::Plan {
            version: 1,
            operation: ID.into(),
            cloud_operation: ID.into(),
            cloud_hash: "a".repeat(64),
            source: "b".repeat(64),
            main_key: "custom/tasks.json".into(),
            files: vec![f.clone()],
            objects: backup::cloud::object_keys("custom/tasks.json")
                .unwrap()
                .into_iter()
                .enumerate()
                .map(|(i, key)| backup::cloud::ObjectEntry {
                    key,
                    etag: if i == 0 {
                        Some("\"etag\"".into())
                    } else {
                        None
                    },
                    file: if i == 0 { Some(f.clone()) } else { None },
                })
                .collect(),
            confirmed: true,
            completed: vec![f.name.clone()],
            published: true,
        };
        let mut c = Cloud::default();
        c.objects
            .insert(seed::marker_key(&p).unwrap(), seed::manifest(&p).unwrap());
        c.objects.insert(seed::object_key(&p, &f).unwrap(), bytes);
        (Claim::from_plan(&p).unwrap(), c)
    }
    #[test]
    fn initialization_resumes_at_every_request_and_lost_response() {
        let (claim, cloud) = seed_fixture();
        let mut complete = cloud.clone();
        initialize(&mut complete, &claim, || Ok(())).unwrap();
        let total = complete.calls;
        for after in [false, true] {
            for fail in 1..=total {
                let mut c = cloud.clone();
                c.fail = fail;
                c.after = after;
                assert!(
                    initialize(&mut c, &claim, || Ok(())).is_err(),
                    "fault {fail}/{after}"
                );
                c.fail = 0;
                initialize(&mut c, &claim, || Ok(())).unwrap();
                assert_eq!(c.objects, complete.objects);
            }
        }
    }
    #[test]
    fn ready_join_does_not_replay_seed_and_claim_collision_does_not_switch() {
        let (claim, mut cloud) = seed_fixture();
        initialize(&mut cloud, &claim, || Ok(())).unwrap();
        let main = claim.main().unwrap();
        let mut newer: Value = serde_json::from_str(&empty(0, ID).unwrap()).unwrap();
        newer["generated_at"] = 123.into();
        cloud.objects.insert(
            main.clone(),
            sync_space::encode(&main, &newer.to_string())
                .unwrap()
                .into_bytes(),
        );
        let before = cloud.objects.clone();
        initialize(&mut cloud, &claim, || Ok(())).unwrap();
        assert_eq!(cloud.objects, before);
        let mut p = claim.plan().unwrap();
        p.operation = "00000000-0000-4000-8000-000000000002".into();
        assert_eq!(
            initialize(&mut cloud, &Claim::from_plan(&p).unwrap(), || Ok(()))
                .err()
                .as_deref(),
            Some("MIGRATION_SPACE_CHANGED")
        );
        assert_eq!(cloud.objects, before);
        cloud.objects.remove(&main);
        assert_eq!(
            initialize(&mut cloud, &claim, || Ok(())).err().as_deref(),
            Some("SYNC_SPACE_INCOMPLETE")
        );
        assert!(!cloud.objects.contains_key(&main));
    }
    #[test]
    fn invalid_seed_never_reserves_source_and_dependencies_are_checked() {
        let (claim, mut cloud) = seed_fixture();
        let mut p = claim.plan().unwrap();
        let bytes = b"{}";
        let f = FileEntry {
            name: "meta-0.json".into(),
            size: 2,
            sha256: hash(bytes),
        };
        p.files = vec![f.clone()];
        p.objects[0].file = Some(f.clone());
        cloud
            .objects
            .insert(seed::marker_key(&p).unwrap(), seed::manifest(&p).unwrap());
        cloud
            .objects
            .insert(seed::object_key(&p, &f).unwrap(), bytes.to_vec());
        let invalid_claim = Claim::from_plan(&p).unwrap();
        assert!(initialize(&mut cloud, &invalid_claim, || Ok(())).is_err());
        assert!(!cloud.objects.contains_key(&claim_key(&claim.1)));
        let mut docs: Vec<Value> = (0..8)
            .map(|i| serde_json::from_str(&empty(i, ID).unwrap()).unwrap())
            .collect();
        docs[4]["links"] = json!([{"todo_uuid":ID,"note_uuid":ID,"deleted_at":null}]);
        assert_eq!(
            validate_seed_references(&docs).unwrap_err(),
            "MIGRATION_SPACE_REFERENCES"
        );
        docs[4]["links"][0]["deleted_at"] = 1.into();
        validate_seed_references(&docs).unwrap();
    }
    #[test]
    fn activation_is_atomic_and_manual_key_is_not_proof() {
        let (claim, _) = seed_fixture();
        let mut db = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut db).unwrap();
        db.execute("UPDATE sync_settings SET endpoint='https://isolated.invalid',bucket='fixture',object_key='custom/tasks.json'",[]).unwrap();
        let mut plan = claim.plan().unwrap();
        plan.source = crate::s3_sync::migration_source_binding(&db).unwrap();
        let claim = Claim::from_plan(&plan).unwrap();
        let work = backup::prepare(&mut db, 100).unwrap();
        backup::finish(&mut db, &work.plan, 101).unwrap();
        let pending = Pending {
            claim: claim.clone(),
            local: backup::latest(&db).unwrap().unwrap(),
            mode: "create".into(),
        };
        save_pending(&db, &pending).unwrap();
        assert!(!is_active(&db).unwrap());
        assert_eq!(
            admit(&db, &claim.main().unwrap()).unwrap_err(),
            "SYNC_SPACE_ACTIVATION_REQUIRED"
        );
        let invalid = lifecycle_sync::Document {
            format_version: 2,
            terminals: vec![],
        };
        assert!(activate(&mut db, &pending, &invalid).is_err());
        assert_eq!(
            db.query_row("SELECT object_key FROM sync_settings", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "custom/tasks.json"
        );
        assert!(!is_active(&db).unwrap());
        assert!(super::pending(&db).unwrap().is_some());
        activate(
            &mut db,
            &pending,
            &lifecycle_sync::Document {
                format_version: 1,
                terminals: vec![],
            },
        )
        .unwrap();
        assert!(is_active(&db).unwrap());
        crate::purge::require_safe(&db).unwrap();
        crate::sync_target::invalidate(&db).unwrap();
        crate::sync_target::activate(&db).unwrap();
        assert!(is_active(&db).unwrap());
        db.execute("UPDATE sync_settings SET bucket='other'", [])
            .unwrap();
        assert!(!is_active(&db).unwrap());
        assert!(crate::purge::require_safe(&db).is_err());
    }
}
