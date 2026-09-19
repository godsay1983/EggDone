//! Immutable migration seed publication. This never switches the active synchronization space.
use super::*;
const KEY: &str = "migration.backup.publication.v1";
pub const FORMAT: &str = "eggdone.migration-seed.v1";
pub const MISSING_FORMAT: &str = "eggdone.migration-seed.v2";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Plan {
    pub version: u32,
    pub operation: String,
    pub cloud_operation: String,
    pub cloud_hash: String,
    pub source: String,
    pub main_key: String,
    pub files: Vec<FileEntry>,
    pub objects: Vec<cloud::ObjectEntry>,
    pub confirmed: bool,
    pub completed: Vec<String>,
    pub published: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub operation: String,
    pub digest: String,
    pub total: usize,
    pub completed: usize,
    pub bytes: u64,
    pub confirmed: bool,
    pub published: bool,
    pub current: bool,
}

pub fn prefix(main: &str, operation: &str) -> Result<String, String> {
    cloud::object_keys(main)?;
    if !id(operation) {
        return Err("MIGRATION_PUBLICATION_INVALID".into());
    }
    Ok(format!(
        "eggdone-migration-staging/v1/{}/{operation}/",
        digest(main.as_bytes())
    ))
}
pub fn object_key(plan: &Plan, file: &FileEntry) -> Result<String, String> {
    if !plan.files.contains(file) {
        return Err("MIGRATION_PUBLICATION_INVALID".into());
    }
    Ok(format!(
        "{}objects/{}",
        prefix(&plan.main_key, &plan.operation)?,
        file.sha256
    ))
}
pub fn marker_key(plan: &Plan) -> Result<String, String> {
    Ok(format!(
        "{}manifest.json",
        prefix(&plan.main_key, &plan.operation)?
    ))
}
pub fn manifest(plan: &Plan) -> Result<Vec<u8>, String> {
    // Arrays fix serialization order across Rust/ArkTS. No local rows or credentials are published.
    let objects: Vec<serde_json::Value> = plan
        .objects
        .iter()
        .map(|o| {
            serde_json::json!([
                o.key,
                o.etag,
                o.file.as_ref().map(|f| &f.name),
                o.file.as_ref().map(|f| f.size),
                o.file.as_ref().map(|f| &f.sha256)
            ])
        })
        .collect();
    let has_missing = plan.files.iter().any(|f| f.missing);
    let files: Vec<serde_json::Value> = plan
        .files
        .iter()
        .map(|f| {
            if has_missing {
                serde_json::json!([f.name, f.size, f.sha256, f.missing])
            } else {
                serde_json::json!([f.name, f.size, f.sha256])
            }
        })
        .collect();
    serde_json::to_vec(&serde_json::json!([
        if has_missing { MISSING_FORMAT } else { FORMAT },
        plan.operation,
        plan.source,
        plan.cloud_hash,
        objects,
        files
    ]))
    .map_err(|_| "MIGRATION_PUBLICATION_INVALID".into())
}
pub fn plan_digest(plan: &Plan) -> Result<String, String> {
    Ok(digest(&manifest(plan)?))
}
pub(crate) fn validate(plan: &Plan) -> Result<(), String> {
    if plan.version != 1
        || !id(&plan.operation)
        || !id(&plan.cloud_operation)
        || !sha(&plan.cloud_hash)
        || !sha(&plan.source)
        || plan.files.is_empty()
        || plan.files.len() > 10000
        || plan.objects.len() != 8
        || plan.objects[0].file.is_none()
        || plan.published && !plan.confirmed
    {
        return Err("MIGRATION_PUBLICATION_INVALID".into());
    }
    let keys = cloud::object_keys(&plan.main_key)?;
    let mut names = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for f in &plan.files {
        let meta = (0..8).any(|i| f.name == format!("meta-{i}.json"));
        let asset = f.name.is_ascii()
            && f.name.len() > 37
            && id(&f.name[..36])
            && ["-original", "-preview.jpg"].contains(&&f.name[36..]);
        if (!meta && !asset)
            || (meta && f.missing)
            || !sha(&f.sha256)
            || f.size == 0
            || f.size > 20 * 1024 * 1024
            || !names.insert(&f.name)
        {
            return Err("MIGRATION_PUBLICATION_INVALID".into());
        }
        total += f.size;
    }
    for (i, o) in plan.objects.iter().enumerate() {
        if o.key != keys[i]
            || (o.etag.is_none() != o.file.is_none())
            || o.etag.as_ref().is_some_and(|e| !cloud::valid_etag(e))
            || o.file
                .as_ref()
                .is_some_and(|f| f.name != format!("meta-{i}.json") || !plan.files.contains(f))
        {
            return Err("MIGRATION_PUBLICATION_INVALID".into());
        }
    }
    let done: std::collections::BTreeSet<_> = plan.completed.iter().collect();
    if done.len() != plan.completed.len()
        || done.iter().any(|s| !names.contains(*s))
        || (!plan.confirmed && !done.is_empty())
        || (plan.published && done.len() != names.len())
        || total > MAX_BYTES
        || manifest(plan)?.len() > cloud::MAX_OBJECT
    {
        return Err("MIGRATION_PUBLICATION_INVALID".into());
    }
    Ok(())
}
pub fn latest(c: &Connection) -> Result<Option<Plan>, String> {
    let raw: Option<String> = c
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db)?;
    raw.map(|s| {
        let p: Plan = serde_json::from_str(&s).map_err(|_| "MIGRATION_PUBLICATION_INVALID")?;
        validate(&p)?;
        Ok(p)
    })
    .transpose()
}
fn save(c: &Connection, p: &Plan) -> Result<(), String> {
    validate(p)?;
    c.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [KEY,&serde_json::to_string(p).map_err(|_|"MIGRATION_PUBLICATION_INVALID")?]).map_err(db)?;
    Ok(())
}
pub fn require_current(
    c: &Connection,
    local: &BackupPlan,
    cloud: &cloud::Plan,
    p: &Plan,
) -> Result<(), String> {
    validate(p)?;
    cloud::require_current(c, local, cloud)?;
    if cloud.verified_at.is_none()
        || p.cloud_operation != cloud.operation
        || p.cloud_hash != digest(&cloud::manifest_cloud(cloud)?)
        || p.source != cloud.source
        || p.main_key != cloud.main_key
        || p.objects != cloud.objects
        || p.files != cloud::files(cloud).cloned().collect::<Vec<_>>()
    {
        return Err("MIGRATION_PUBLICATION_CHANGED".into());
    }
    let saved = latest(c)?.ok_or("MIGRATION_PUBLICATION_CHANGED")?;
    if plan_digest(&saved)? != plan_digest(p)? || saved.cloud_operation != p.cloud_operation {
        return Err("MIGRATION_PUBLICATION_CHANGED".into());
    }
    Ok(())
}
pub fn prepare(
    c: &mut Connection,
    local: &BackupPlan,
    cloud: &cloud::Plan,
) -> Result<Plan, String> {
    if !crate::migration_preflight::read(c)?.blockers().is_empty() {
        return Err("MIGRATION_LOCAL_NOT_SETTLED".into());
    }
    let tx = c.transaction().map_err(db)?;
    cloud::require_current(&tx, local, cloud)?;
    if cloud.verified_at.is_none() || cloud.objects[0].file.is_none() {
        return Err("MIGRATION_PUBLICATION_UNVERIFIED".into());
    }
    let hash = digest(&cloud::manifest_cloud(cloud)?);
    let p = match latest(&tx)? {
        Some(p) if p.cloud_operation == cloud.operation && p.cloud_hash == hash => p,
        _ => Plan {
            version: 1,
            operation: uuid::Uuid::new_v4().to_string(),
            cloud_operation: cloud.operation.clone(),
            cloud_hash: hash,
            source: cloud.source.clone(),
            main_key: cloud.main_key.clone(),
            files: cloud::files(cloud).cloned().collect(),
            objects: cloud.objects.clone(),
            confirmed: false,
            completed: Vec::new(),
            published: false,
        },
    };
    save(&tx, &p)?;
    tx.commit().map_err(db)?;
    Ok(p)
}
pub fn record(
    c: &mut Connection,
    local: &BackupPlan,
    cloud: &cloud::Plan,
    p: &Plan,
    expected: &str,
    done: Option<&str>,
    published: bool,
) -> Result<(), String> {
    let tx = c.transaction().map_err(db)?;
    require_current(&tx, local, cloud, p)?;
    if plan_digest(p)? != expected {
        return Err("MIGRATION_PUBLICATION_CONFIRMATION".into());
    }
    let mut current = latest(&tx)?.ok_or("MIGRATION_PUBLICATION_CHANGED")?;
    current.confirmed = true;
    if let Some(name) = done {
        if !current.completed.iter().any(|n| n == name) {
            current.completed.push(name.into());
            current.completed.sort();
        }
    }
    if published {
        current.published = true;
    }
    save(&tx, &current)?;
    tx.commit().map_err(db)
}
pub trait Storage {
    fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String>;
    fn create(&mut self, key: &str, bytes: &[u8]) -> Result<(), String>;
}
pub fn publish(
    plan: &Plan,
    storage: &mut impl Storage,
    mut source: impl FnMut(&FileEntry) -> Result<Vec<u8>, String>,
    mut guard: impl FnMut() -> Result<(), String>,
    mut recheck: impl FnMut() -> Result<(), String>,
    mut progress: impl FnMut(Option<&str>, bool) -> Result<(), String>,
) -> Result<(), String> {
    validate(plan)?;
    if !plan.confirmed {
        return Err("MIGRATION_PUBLICATION_CONFIRMATION".into());
    }
    guard()?;
    let marker = marker_key(plan)?;
    let manifest = manifest(plan)?;
    let existing = storage.read(&marker, cloud::MAX_OBJECT)?;
    guard()?;
    if existing.as_ref().is_some_and(|b| b != &manifest) {
        return Err("MIGRATION_PUBLICATION_CONFLICT".into());
    }
    let sealed = existing.is_some() || plan.published;
    if plan.published && existing.is_none() {
        return Err("MIGRATION_PUBLICATION_DAMAGED".into());
    }
    recheck()?;
    for entry in &plan.files {
        if entry.missing {
            guard()?;
            progress(Some(&entry.name), false)?;
            continue;
        }
        guard()?;
        let key = object_key(plan, entry)?;
        let mut bytes = storage.read(&key, entry.size as usize)?;
        guard()?;
        if bytes.is_none() {
            if sealed {
                return Err("MIGRATION_PUBLICATION_DAMAGED".into());
            }
            let local = source(entry)?;
            if local.len() as u64 != entry.size || digest(&local) != entry.sha256 {
                return Err("MIGRATION_BACKUP_FILE".into());
            }
            guard()?;
            storage.create(&key, &local)?;
            bytes = storage.read(&key, entry.size as usize)?;
        }
        guard()?;
        let bytes = bytes.ok_or("MIGRATION_PUBLICATION_DAMAGED")?;
        if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
            return Err("MIGRATION_PUBLICATION_CONFLICT".into());
        }
        progress(Some(&entry.name), false)?;
    }
    recheck()?;
    guard()?;
    if !sealed {
        storage.create(&marker, &manifest)?;
    }
    if storage.read(&marker, cloud::MAX_OBJECT)?.as_deref() != Some(manifest.as_slice()) {
        return Err("MIGRATION_PUBLICATION_CONFLICT".into());
    }
    // The journal is progress, not proof. Verify all targets again after observing the marker.
    for f in &plan.files {
        if f.missing {
            continue;
        }
        guard()?;
        let bytes = storage
            .read(&object_key(plan, f)?, f.size as usize)?
            .ok_or("MIGRATION_PUBLICATION_DAMAGED")?;
        if bytes.len() as u64 != f.size || digest(&bytes) != f.sha256 {
            return Err("MIGRATION_PUBLICATION_DAMAGED".into());
        }
    }
    guard()?;
    progress(None, true)
}
pub fn report(c: &Connection, local: &BackupPlan) -> Result<Option<Report>, String> {
    latest(c)?
        .map(|p| {
            let current = cloud::latest(c)?
                .is_some_and(|cloud| require_current(c, local, &cloud, &p).is_ok());
            Ok(Report {
                operation: p.operation.clone(),
                digest: plan_digest(&p)?,
                total: p.files.len(),
                completed: p.completed.len(),
                bytes: p.files.iter().map(|f| f.size).sum(),
                confirmed: p.confirmed,
                published: p.published,
                current,
            })
        })
        .transpose()
}

#[cfg(test)]
#[path = "migration_publication_tests.rs"]
mod tests;
