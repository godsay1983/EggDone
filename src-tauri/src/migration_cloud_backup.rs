//! Read-only legacy cloud snapshot, bound to a verified private local recovery plan.
use super::*;
const KEY: &str = "migration.backup.cloud.v1";
pub const MAX_OBJECT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObjectEntry {
    pub key: String,
    pub etag: Option<String>,
    pub file: Option<FileEntry>,
}
pub struct RemoteObject {
    pub etag: Option<String>,
    pub bytes: Option<Vec<u8>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Plan {
    version: u32,
    pub operation: String,
    local_operation: String,
    local_hash: String,
    pub(crate) source: String,
    pub(crate) main_key: String,
    created_at: i64,
    pub objects: Vec<ObjectEntry>,
    pub assets: Vec<FileEntry>,
    pub(crate) verified_at: Option<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    operation: String,
    objects: usize,
    files: usize,
    bytes: u64,
    verified_at: Option<i64>,
    current: bool,
}

pub fn object_keys(main: &str) -> Result<Vec<String>, String> {
    let keys = vec![
        main.into(),
        crate::s3_sync::derive_note_object_key(main),
        crate::s3_sync::derive_note_attachment_object_key(main),
        crate::recurrence_protocol::recurrence_object_key(main, &[])?,
        crate::task_note_link_protocol::object_key(main, &[])?,
        crate::task_checklist_sync::Domain::Items.object_key(main, &[])?,
        crate::task_checklist_sync::Domain::Definitions.object_key(main, &[])?,
        crate::task_template_sync::object_key(main, &[])?,
    ];
    if keys.iter().collect::<std::collections::BTreeSet<_>>().len() != 8 {
        return Err("MIGRATION_CLOUD_INVALID".into());
    }
    Ok(keys)
}
pub fn valid_etag(value: &str) -> bool {
    let b = value.as_bytes();
    (3..=1024).contains(&b.len())
        && b[0] == b'"'
        && b[b.len() - 1] == b'"'
        && b[1..b.len() - 1]
            .iter()
            .all(|n| *n == 0x21 || (0x23..=0x7e).contains(n))
}
pub fn validate_document(index: usize, bytes: &[u8]) -> Result<(), String> {
    let parse = || -> Result<(), String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "MIGRATION_CLOUD_INVALID")?;
        let json = |e: serde_json::Error| e.to_string();
        match index {
            0 => crate::sync::validate_document(&serde_json::from_str(text).map_err(json)?),
            1 => crate::note_sync::validate_document(&serde_json::from_str(text).map_err(json)?),
            2 => crate::note_attachment_sync::validate_document(
                &serde_json::from_str(text).map_err(json)?,
            ),
            3 => crate::recurrence_protocol::parse_document(text).map(|_| ()),
            4 => crate::task_note_link_protocol::parse_document(text).map(|_| ()),
            5 => crate::task_checklist_protocol::parse_items(text).map(|_| ()),
            6 => crate::task_checklist_protocol::parse_definitions(text).map(|_| ()),
            7 => crate::task_template_protocol::parse(text).map(|_| ()),
            _ => Err("MIGRATION_CLOUD_INVALID".into()),
        }
    };
    if bytes.len() > MAX_OBJECT {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    parse().map_err(|_| "MIGRATION_CLOUD_INVALID".into())
}
fn file(name: String, bytes: &[u8]) -> FileEntry {
    FileEntry {
        name,
        size: bytes.len() as u64,
        sha256: digest(bytes),
    }
}
fn entries(
    main: &str,
    remote: &[RemoteObject],
) -> Result<(Vec<ObjectEntry>, Vec<FileEntry>), String> {
    if remote.len() != 8 {
        return Err("MIGRATION_CLOUD_INVALID".into());
    }
    let mut objects = Vec::new();
    let mut assets = Vec::new();
    let mut total = 0;
    for (i, (key, object)) in object_keys(main)?.into_iter().zip(remote).enumerate() {
        let entry = match (&object.bytes, &object.etag) {
            (None, None) => None,
            (Some(bytes), Some(etag)) if valid_etag(etag) => {
                validate_document(i, bytes)?;
                total += bytes.len();
                if total > MAX_OBJECT {
                    return Err("MIGRATION_BACKUP_LIMIT".into());
                }
                if i == 2 {
                    let document: crate::note_attachment_sync::NoteAttachmentSyncDocument =
                        serde_json::from_slice(bytes).map_err(|_| "MIGRATION_CLOUD_INVALID")?;
                    for a in document.attachments {
                        assets.push(FileEntry {
                            name: format!("{}-original", a.uuid),
                            size: a.byte_size as u64,
                            sha256: a.sha256,
                        });
                        if a.kind == "image" {
                            assets.push(FileEntry {
                                name: format!("{}-preview.jpg", a.uuid),
                                size: a.preview_byte_size.ok_or("MIGRATION_CLOUD_INVALID")? as u64,
                                sha256: a.preview_sha256.ok_or("MIGRATION_CLOUD_INVALID")?,
                            });
                        }
                    }
                }
                Some(file(format!("meta-{i}.json"), bytes))
            }
            _ => return Err("MIGRATION_CLOUD_INVALID".into()),
        };
        objects.push(ObjectEntry {
            key,
            etag: object.etag.clone(),
            file: entry,
        });
    }
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((objects, assets))
}
fn validate_cloud(plan: &Plan) -> Result<(), String> {
    if plan.version != 1
        || !id(&plan.operation)
        || !id(&plan.local_operation)
        || !sha(&plan.local_hash)
        || !sha(&plan.source)
        || !(0..=9_007_199_254_740_991).contains(&plan.created_at)
        || plan
            .verified_at
            .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
        || plan.objects.len() != 8
        || plan.assets.len() >= 10000
    {
        return Err("MIGRATION_CLOUD_INVALID".into());
    }
    let keys = object_keys(&plan.main_key)?;
    let mut total = 0;
    for (i, entry) in plan.objects.iter().enumerate() {
        if entry.key != keys[i] {
            return Err("MIGRATION_CLOUD_INVALID".into());
        }
        match (&entry.etag, &entry.file) {
            (None, None) => (),
            (Some(etag), Some(f))
                if valid_etag(etag)
                    && f.name == format!("meta-{i}.json")
                    && f.size > 0
                    && f.size <= MAX_OBJECT as u64
                    && sha(&f.sha256) =>
            {
                total += f.size;
            }
            _ => return Err("MIGRATION_CLOUD_INVALID".into()),
        }
    }
    if total > MAX_OBJECT as u64 {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in &plan.assets {
        if !f.name.is_ascii()
            || f.name.len() <= 37
            || !id(&f.name[..36])
            || !["original", "preview.jpg"].contains(&&f.name[37..])
            || &f.name[36..37] != "-"
            || !sha(&f.sha256)
            || f.size == 0
            || f.size > 20 * 1024 * 1024
            || !seen.insert(&f.name)
        {
            return Err("MIGRATION_CLOUD_INVALID".into());
        }
        total += f.size;
    }
    if total > MAX_BYTES {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    Ok(())
}
pub(crate) fn files(plan: &Plan) -> impl Iterator<Item = &FileEntry> {
    plan.objects
        .iter()
        .filter_map(|o| o.file.as_ref())
        .chain(plan.assets.iter())
}
fn bounded(plan: &Plan, local: &BackupPlan) -> Result<(), String> {
    validate_cloud(plan)?;
    let bytes =
        files(plan).map(|f| f.size).sum::<u64>() + local.files.iter().map(|f| f.size).sum::<u64>();
    let overhead = manifest_cloud(plan)?.len() as u64 + manifest(local)?.len() as u64;
    if bytes + overhead > MAX_BYTES || files(plan).count() + local.files.len() + 2 > 10000 {
        return Err("MIGRATION_BACKUP_LIMIT".into());
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
        let p: Plan = serde_json::from_str(&s).map_err(|_| "MIGRATION_CLOUD_INVALID")?;
        validate_cloud(&p)?;
        Ok(p)
    })
    .transpose()
}
fn save(c: &Connection, plan: &Plan) -> Result<(), String> {
    let text = serde_json::to_string(plan).map_err(|_| "MIGRATION_CLOUD_INVALID")?;
    c.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [KEY, &text]).map_err(db)?;
    Ok(())
}
pub fn prepare(
    c: &mut Connection,
    local: &BackupPlan,
    source: &str,
    main: &str,
    remote: &[RemoteObject],
    now: i64,
) -> Result<Plan, String> {
    let (objects, assets) = entries(main, remote)?;
    let tx = c.transaction().map_err(db)?;
    super::require_current(&tx, local)?;
    if local.verified_at.is_none() {
        return Err("MIGRATION_CLOUD_LOCAL_REQUIRED".into());
    }
    if crate::s3_sync::migration_source_binding(&tx)? != source {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    let previous = latest(&tx)?;
    let mut plan = match previous {
        Some(p)
            if p.local_operation == local.operation
                && p.local_hash == local.data_hash
                && p.source == source
                && p.main_key == main
                && p.objects == objects
                && p.assets == assets =>
        {
            p
        }
        _ => Plan {
            version: 1,
            operation: uuid::Uuid::new_v4().to_string(),
            local_operation: local.operation.clone(),
            local_hash: local.data_hash.clone(),
            source: source.into(),
            main_key: main.into(),
            created_at: now,
            objects,
            assets,
            verified_at: None,
        },
    };
    plan.verified_at = None;
    bounded(&plan, local)?;
    save(&tx, &plan)?;
    tx.commit().map_err(db)?;
    Ok(plan)
}
pub fn require_current(c: &Connection, local: &BackupPlan, plan: &Plan) -> Result<(), String> {
    super::require_current(c, local)?;
    if plan.local_operation != local.operation
        || plan.local_hash != local.data_hash
        || local.verified_at.is_none()
        || plan.source != crate::s3_sync::migration_source_binding(c)?
    {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    let current = latest(c)?.ok_or("MIGRATION_CLOUD_CHANGED")?;
    if manifest_cloud(&current)? != manifest_cloud(plan)? {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    Ok(())
}
pub fn finish(c: &mut Connection, local: &BackupPlan, plan: &Plan, now: i64) -> Result<(), String> {
    let tx = c.transaction().map_err(db)?;
    require_current(&tx, local, plan)?;
    let mut verified = plan.clone();
    verified.verified_at = Some(now);
    save(&tx, &verified)?;
    tx.commit().map_err(db)
}
fn cloud_folder(
    root: &Path,
    local: &BackupPlan,
    plan: &Plan,
    create: bool,
) -> Result<PathBuf, String> {
    bounded(plan, local)?;
    if plan.local_operation != local.operation || plan.local_hash != local.data_hash {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    let target = folder(root, local, false)?.join(format!("cloud-{}", plan.operation));
    directory(&target, create)?;
    Ok(target)
}
pub(crate) fn manifest_cloud(plan: &Plan) -> Result<Vec<u8>, String> {
    let mut copy = plan.clone();
    copy.verified_at = None;
    serde_json::to_vec(&copy).map_err(|_| "MIGRATION_CLOUD_INVALID".into())
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    writable(path)?;
    let mut file = fs::File::create(path).map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
pub fn copy(
    root: &Path,
    local: &BackupPlan,
    plan: &Plan,
    remote: &[RemoteObject],
    mut download: impl FnMut(&FileEntry) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let (objects, assets) = entries(&plan.main_key, remote)?;
    if objects != plan.objects || assets != plan.assets {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    let target = cloud_folder(root, local, plan, true)?;
    write(&target.join("manifest.json"), &manifest_cloud(plan)?)?;
    for (object, data) in plan.objects.iter().zip(remote) {
        if let (Some(entry), Some(bytes)) = (&object.file, &data.bytes) {
            let path = target.join(&entry.name);
            if verify(&path, entry).is_err() {
                write(&path, bytes)?;
            }
            verify(&path, entry)?;
        }
    }
    for entry in &plan.assets {
        let output = target.join(&entry.name);
        if verify(&output, entry).is_ok() {
            continue;
        }
        writable(&output)?;
        let bytes = download(entry)?;
        if file(entry.name.clone(), &bytes) != *entry {
            return Err("MIGRATION_BACKUP_ASSET_INVALID".into());
        }
        write(&output, &bytes)?;
        verify(&output, entry)?;
    }
    verify_files(root, local, plan)
}
pub fn verify_files(root: &Path, local: &BackupPlan, plan: &Plan) -> Result<(), String> {
    let target = cloud_folder(root, local, plan, false)?;
    verify(
        &target.join("manifest.json"),
        &file("manifest.json".into(), &manifest_cloud(plan)?),
    )?;
    for entry in files(plan) {
        verify(&target.join(&entry.name), entry)?;
    }
    Ok(())
}
pub fn read_file(
    root: &Path,
    local: &BackupPlan,
    plan: &Plan,
    entry: &FileEntry,
) -> Result<Vec<u8>, String> {
    if !files(plan).any(|f| f == entry) {
        return Err("MIGRATION_PUBLICATION_INVALID".into());
    }
    let path = cloud_folder(root, local, plan, false)?.join(&entry.name);
    verify(&path, entry)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(io)?
        .take(entry.size + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
        return Err("MIGRATION_BACKUP_FILE".into());
    }
    Ok(bytes)
}
pub fn require_remote(plan: &Plan, remote: &[RemoteObject]) -> Result<(), String> {
    let (objects, assets) = entries(&plan.main_key, remote)?;
    if plan.objects != objects || plan.assets != assets {
        return Err("MIGRATION_CLOUD_CHANGED".into());
    }
    Ok(())
}
pub fn report(c: &Connection, local: &BackupPlan) -> Result<Option<Report>, String> {
    Ok(latest(c)?.map(|plan| Report {
        operation: plan.operation.clone(),
        objects: plan.objects.iter().filter(|o| o.file.is_some()).count(),
        files: plan.assets.len(),
        bytes: files(&plan).map(|f| f.size).sum(),
        verified_at: plan.verified_at,
        current: plan.local_operation == local.operation
            && plan.local_hash == local.data_hash
            && local.verified_at.is_some()
            && super::require_current(c, local).is_ok()
            && crate::s3_sync::migration_source_binding(c).is_ok_and(|b| b == plan.source),
    }))
}

#[cfg(test)]
#[path = "migration_cloud_backup_tests.rs"]
mod tests;
