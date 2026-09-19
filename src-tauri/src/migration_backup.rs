//! Private, same-client recovery snapshot. Never a cloud publication or portable import format.
use rusqlite::{types::ValueRef, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const KEY: &str = "migration.backup.v1";
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DATA: usize = 16 * 1024 * 1024;
const TABLES: &[&str] = &[
    "todos",
    "groups",
    "notes",
    "note_attachments",
    "recurrence_rules",
    "task_note_links",
    "task_checklist_items",
    "task_checklist_definitions",
    "task_templates",
    "note_history",
    "task_checklist_operations",
    "task_template_operations",
    "lifecycle_terminals",
    "lifecycle_sync_state",
    "purge_plans",
    "purge_targets",
    "purge_cleanup",
    "sync_runtime_state",
    "recurrence_sync_state",
    "task_note_link_sync_state",
    "task_checklist_sync_state",
    "task_template_sync_state",
    "sync_settings",
    "app_metadata",
    "daily_plan_events",
    "daily_plans",
    "daily_plan_completions",
    "daily_plan_operations",
    "daily_plan_sync_state",
    "task_workflow_states",
    "task_workflow_operations",
    "task_workflow_sync_state",
];

fn tables_for_schema(schema: i64) -> &'static [&'static str] {
    if schema < 24 {
        &TABLES[..TABLES.len() - 8]
    } else if schema < 26 {
        &TABLES[..TABLES.len() - 3]
    } else {
        TABLES
    }
}

#[path = "migration_recovery.rs"]
mod recovery;
pub use recovery::rehearse;
#[path = "migration_cloud_backup.rs"]
pub mod cloud;
#[path = "migration_publication.rs"]
pub mod publication;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "not_missing")]
    pub missing: bool,
}
fn not_missing(value: &bool) -> bool {
    !value
}
pub(crate) fn identities(files: &[FileEntry]) -> Vec<FileEntry> {
    files
        .iter()
        .cloned()
        .map(|mut f| {
            f.missing = false;
            f
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BackupPlan {
    version: u32,
    pub operation: String,
    created_at: i64,
    data_hash: String,
    files: Vec<FileEntry>,
    pub verified_at: Option<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupReport {
    pub publication: Option<publication::Report>,
    pub operation: String,
    pub files: usize,
    pub bytes: u64,
    pub verified_at: Option<i64>,
    pub current: bool,
    pub blockers: Vec<String>,
    pub cloud: Option<cloud::Report>,
}
pub struct Work {
    pub plan: BackupPlan,
    data: Vec<u8>,
}

fn db(_: rusqlite::Error) -> String {
    "MIGRATION_BACKUP_DATABASE".into()
}
fn io(_: std::io::Error) -> String {
    "MIGRATION_BACKUP_IO".into()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|v| v.to_string() == value)
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// Fixed table names only; no SQL or file paths are accepted from the UI or a saved plan.
fn capture(c: &Connection) -> Result<Vec<u8>, String> {
    let schema: i64 = c
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .map_err(db)?;
    if ![22, 23, 24, 25, 26].contains(&schema) {
        return Err("MIGRATION_BACKUP_INVALID".into());
    }
    let mut tables = Vec::new();
    let mut bytes_read = 0usize;
    for table in tables_for_schema(schema) {
        let sql = if *table == "app_metadata" {
            "SELECT * FROM app_metadata WHERE key NOT LIKE 'migration.backup.%' ORDER BY key".into()
        } else {
            format!("SELECT * FROM {table} ORDER BY rowid")
        };
        let mut statement = c.prepare(&sql).map_err(db)?;
        let columns = statement
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let width = columns.len();
        let mut rows = statement.query([]).map_err(db)?;
        let mut values = Vec::new();
        while let Some(row) = rows.next().map_err(db)? {
            let mut cells = Vec::new();
            for i in 0..width {
                cells.push(match row.get_ref(i).map_err(db)? {
                    ValueRef::Null => serde_json::Value::Null,
                    ValueRef::Integer(n)
                        if (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n) =>
                    {
                        n.into()
                    }
                    ValueRef::Text(s) => std::str::from_utf8(s)
                        .map_err(|_| "MIGRATION_BACKUP_INVALID")?
                        .into(),
                    _ => return Err("MIGRATION_BACKUP_INVALID".into()),
                });
            }
            bytes_read += serde_json::to_vec(&cells)
                .map_err(|_| "MIGRATION_BACKUP_INVALID")?
                .len();
            if bytes_read > MAX_DATA {
                return Err("MIGRATION_BACKUP_LIMIT".into());
            }
            values.push(cells);
            if values.len() > 100_000 {
                return Err("MIGRATION_BACKUP_LIMIT".into());
            }
        }
        tables.push(serde_json::json!({"name":table,"columns":columns,"rows":values}));
    }
    let bytes = serde_json::to_vec(&serde_json::json!({"format":"eggdone.local-migration-recovery.v1","client":"desktop","schema":schema,"tables":tables}))
        .map_err(|_| "MIGRATION_BACKUP_INVALID")?;
    if bytes.len() > MAX_DATA {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    Ok(bytes)
}

fn assets(c: &Connection) -> Result<Vec<FileEntry>, String> {
    let mut statement = c.prepare("SELECT uuid,kind,byte_size,sha256,preview_byte_size,preview_sha256 FROM note_attachments ORDER BY uuid").map_err(db)?;
    let mut rows = statement.query([]).map_err(db)?;
    let mut files = Vec::new();
    while let Some(row) = rows.next().map_err(db)? {
        let uuid: String = row.get(0).map_err(db)?;
        if !id(&uuid) {
            return Err("MIGRATION_BACKUP_INVALID".into());
        }
        files.push(FileEntry {
            missing: false,
            name: format!("{uuid}-original"),
            size: row.get(2).map_err(db)?,
            sha256: row.get(3).map_err(db)?,
        });
        if row.get::<_, String>(1).map_err(db)? == "image" {
            files.push(FileEntry {
                missing: false,
                name: format!("{uuid}-preview.jpg"),
                size: row.get(4).map_err(db)?,
                sha256: row.get(5).map_err(db)?,
            });
        }
    }
    Ok(files)
}

fn validate(plan: &BackupPlan) -> Result<(), String> {
    if plan.version != 1
        || !(0..=9_007_199_254_740_991).contains(&plan.created_at)
        || plan
            .verified_at
            .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
        || !id(&plan.operation)
        || !sha(&plan.data_hash)
        || plan.files.is_empty()
        || plan.files.len() >= 10000
    {
        return Err("MIGRATION_BACKUP_INVALID".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (index, file) in plan.files.iter().enumerate() {
        if !file.name.is_ascii() {
            return Err("MIGRATION_BACKUP_INVALID".into());
        }
        let valid_name = if index == 0 {
            file.name == "data.json"
                && file.sha256 == plan.data_hash
                && file.size <= MAX_DATA as u64
        } else {
            file.name.len() > 37
                && id(&file.name[..36])
                && ["-original", "-preview.jpg"].contains(&&file.name[36..])
        };
        if !file.name.is_ascii()
            || (index == 0 && file.missing)
            || !valid_name
            || !sha(&file.sha256)
            || file.size == 0
            || !seen.insert(&file.name)
        {
            return Err("MIGRATION_BACKUP_INVALID".into());
        }
        total = total
            .checked_add(file.size)
            .ok_or("MIGRATION_BACKUP_LIMIT")?;
    }
    if total > MAX_BYTES {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    Ok(())
}
pub fn latest(c: &Connection) -> Result<Option<BackupPlan>, String> {
    let value: Option<String> = c
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(db)?;
    value
        .map(|s| {
            let p: BackupPlan = serde_json::from_str(&s).map_err(|_| "MIGRATION_BACKUP_INVALID")?;
            validate(&p)?;
            Ok(p)
        })
        .transpose()
}
fn save(c: &Connection, plan: &BackupPlan) -> Result<(), String> {
    let json = serde_json::to_string(plan).map_err(|_| "MIGRATION_BACKUP_INVALID")?;
    c.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[KEY,&json]).map_err(db)?;
    Ok(())
}
pub fn prepare(c: &mut Connection, now: i64) -> Result<Work, String> {
    let tx = c.transaction().map_err(db)?;
    let data = capture(&tx)?;
    let data_hash = digest(&data);
    let previous = latest(&tx)?;
    let mut plan = match previous {
        Some(p) if p.data_hash == data_hash => p,
        _ => {
            let mut files = vec![FileEntry {
                missing: false,
                name: "data.json".into(),
                size: data.len() as u64,
                sha256: data_hash.clone(),
            }];
            files.extend(assets(&tx)?);
            BackupPlan {
                version: 1,
                operation: uuid::Uuid::new_v4().to_string(),
                created_at: now,
                data_hash,
                files,
                verified_at: None,
            }
        }
    };
    validate(&plan)?;
    for file in &mut plan.files {
        file.missing = false;
    }
    plan.verified_at = None;
    save(&tx, &plan)?;
    tx.commit().map_err(db)?;
    Ok(Work { plan, data })
}

fn directory(path: &Path, create: bool) -> Result<(), String> {
    if create && !path.try_exists().map_err(io)? {
        fs::create_dir(path).map_err(io)?;
    }
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("MIGRATION_BACKUP_PATH".into());
    }
    #[cfg(unix)]
    if create {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(io)?;
    }
    Ok(())
}
fn folder(root: &Path, plan: &BackupPlan, create: bool) -> Result<PathBuf, String> {
    validate(plan)?;
    directory(root, create)?;
    let target = root.join(&plan.operation);
    directory(&target, create)?;
    Ok(target)
}
fn regular(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("MIGRATION_BACKUP_PATH".into());
    }
    Ok(())
}
fn verify(path: &Path, entry: &FileEntry) -> Result<(), String> {
    regular(path)?;
    let mut file = fs::File::open(path).map_err(io)?;
    if file.metadata().map_err(io)?.len() != entry.size {
        return Err("MIGRATION_BACKUP_FILE".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > entry.size {
            return Err("MIGRATION_BACKUP_FILE".into());
        }
        hash.update(&buffer[..n]);
    }
    if total != entry.size || format!("{:x}", hash.finalize()) != entry.sha256 {
        return Err("MIGRATION_BACKUP_FILE".into());
    }
    Ok(())
}
fn writable(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => regular(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
pub(crate) fn absent(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(io(e)),
    }
}
fn absent_asset(root: &Path, parent: &Path, file: &Path) -> Result<bool, String> {
    if !absent(root)? {
        directory(root, false)?;
    }
    if !absent(parent)? {
        directory(parent, false)?;
    }
    absent(file)
}
#[cfg(test)]
pub fn copy(work: &Work, root: &Path, assets: &Path) -> Result<(), String> {
    copy_with_missing(work, root, assets, |_| Err("MIGRATION_BACKUP_ASSET".into()))
}

// Downloads are only allowed into the private snapshot, never the live attachment cache.
pub fn copy_with_missing(
    work: &Work,
    root: &Path,
    assets: &Path,
    download: impl FnMut(&FileEntry) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let mut copy = Work {
        plan: work.plan.clone(),
        data: work.data.clone(),
    };
    copy_with_policy(&mut copy, root, assets, download, |_| Ok(false))
}
pub fn copy_with_policy(
    work: &mut Work,
    root: &Path,
    assets: &Path,
    mut download: impl FnMut(&FileEntry) -> Result<Vec<u8>, String>,
    mut can_omit: impl FnMut(&FileEntry) -> Result<bool, String>,
) -> Result<(), String> {
    let target = folder(root, &work.plan, true)?;
    let manifest_bytes = manifest(&work.plan)?;
    let manifest_path = target.join("manifest.json");
    writable(&manifest_path)?;
    let mut file = fs::File::create(&manifest_path).map_err(io)?;
    file.write_all(&manifest_bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    for (index, entry) in work.plan.files.iter_mut().enumerate() {
        let output = target.join(&entry.name);
        if verify(&output, entry).is_ok() {
            continue;
        }
        writable(&output)?;
        if index == 0 {
            let mut file = fs::File::create(&output).map_err(io)?;
            file.write_all(&work.data).map_err(io)?;
            file.sync_all().map_err(io)?;
        } else {
            let parent = assets.join(&entry.name[..36]);
            let source = parent.join(&entry.name[37..]);
            let available = directory(assets, false)
                .and_then(|_| directory(&parent, false))
                .and_then(|_| verify(&source, entry));
            if available.is_ok() {
                fs::copy(&source, &output).map_err(io)?;
            } else {
                let bytes = match download(entry) {
                    Ok(bytes) => bytes,
                    Err(error)
                        if error.starts_with("MIGRATION_BACKUP_ASSET_NOT_FOUND")
                            && absent_asset(assets, &parent, &source)?
                            && absent(&output)?
                            && can_omit(entry)? =>
                    {
                        entry.missing = true;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
                    return Err("MIGRATION_BACKUP_ASSET_INVALID".into());
                }
                writable(&output)?;
                let mut file = fs::File::create(&output).map_err(io)?;
                file.write_all(&bytes).map_err(io)?;
                file.sync_all().map_err(io)?;
            }
            fs::OpenOptions::new()
                .write(true)
                .open(&output)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
        }
        verify(&output, entry)?;
    }
    let bytes = manifest(&work.plan)?;
    let mut file = fs::File::create(&manifest_path).map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    verify_files(root, &work.plan)
}
pub fn can_omit(c: &Connection, entry: &FileEntry) -> Result<bool, String> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM note_attachments WHERE uuid=?1 AND deleted_at IS NOT NULL)",
        [&entry.name[..36]],
        |r| r.get(0),
    )
    .map_err(db)
}
pub fn record_missing(
    c: &mut Connection,
    before: &BackupPlan,
    after: &BackupPlan,
) -> Result<(), String> {
    let tx = c.transaction().map_err(db)?;
    require_current(&tx, before)?;
    if before.operation != after.operation
        || before.data_hash != after.data_hash
        || identities(&before.files) != identities(&after.files)
    {
        return Err("MIGRATION_BACKUP_CHANGED".into());
    }
    for file in after.files.iter().filter(|f| f.missing) {
        if !can_omit(&tx, file)? {
            return Err("MIGRATION_BACKUP_CHANGED".into());
        }
    }
    validate(after)?;
    save(&tx, after)?;
    tx.commit().map_err(db)
}
fn manifest(plan: &BackupPlan) -> Result<Vec<u8>, String> {
    let mut copy = plan.clone();
    copy.verified_at = None;
    let bytes = serde_json::to_vec(&copy).map_err(|_| "MIGRATION_BACKUP_INVALID")?;
    if bytes.len() as u64 + plan.files.iter().map(|f| f.size).sum::<u64>() > MAX_BYTES {
        return Err("MIGRATION_BACKUP_LIMIT".into());
    }
    Ok(bytes)
}
pub fn verify_files(root: &Path, plan: &BackupPlan) -> Result<(), String> {
    let target = folder(root, plan, false)?;
    let bytes = manifest(plan)?;
    verify(
        &target.join("manifest.json"),
        &FileEntry {
            missing: false,
            name: "manifest.json".into(),
            size: bytes.len() as u64,
            sha256: digest(&bytes),
        },
    )?;
    for entry in plan.files.iter().filter(|f| !f.missing) {
        verify(&target.join(&entry.name), entry)?;
    }
    Ok(())
}
pub(crate) fn asset_files(plan: &BackupPlan) -> &[FileEntry] {
    &plan.files[1..]
}
pub(crate) fn read_asset(
    root: &Path,
    plan: &BackupPlan,
    entry: &FileEntry,
) -> Result<Vec<u8>, String> {
    if entry.missing || !asset_files(plan).contains(entry) {
        return Err("MIGRATION_BACKUP_FILE".into());
    }
    let path = folder(root, plan, false)?.join(&entry.name);
    verify(&path, entry)?;
    let bytes = fs::read(path).map_err(io)?;
    if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
        return Err("MIGRATION_BACKUP_FILE".into());
    }
    Ok(bytes)
}
pub fn finish(c: &mut Connection, plan: &BackupPlan, now: i64) -> Result<BackupReport, String> {
    let tx = c.transaction().map_err(db)?;
    require_current(&tx, plan)?;
    let mut current = latest(&tx)?.ok_or("MIGRATION_BACKUP_CHANGED")?;
    current.verified_at = Some(now);
    save(&tx, &current)?;
    tx.commit().map_err(db)?;
    report(c, &current)
}
pub fn require_current(c: &Connection, plan: &BackupPlan) -> Result<(), String> {
    let current = latest(c)?.ok_or("MIGRATION_BACKUP_CHANGED")?;
    if current.operation != plan.operation
        || current.files != plan.files
        || assets(c)? != identities(&plan.files[1..])
        || digest(&capture(c)?) != plan.data_hash
    {
        return Err("MIGRATION_BACKUP_CHANGED".into());
    }
    for file in plan.files.iter().filter(|f| f.missing) {
        if !can_omit(c, file)? {
            return Err("MIGRATION_BACKUP_CHANGED".into());
        }
    }
    Ok(())
}
pub fn report(c: &mut Connection, plan: &BackupPlan) -> Result<BackupReport, String> {
    let tx = c.transaction().map_err(db)?;
    let current = digest(&capture(&tx)?) == plan.data_hash;
    tx.commit().map_err(db)?;
    let blockers = crate::migration_preflight::read(c)?.blockers();
    Ok(BackupReport {
        operation: plan.operation.clone(),
        files: plan.files.len() - 1,
        bytes: plan.files.iter().map(|f| f.size).sum(),
        verified_at: plan.verified_at,
        current,
        blockers,
        cloud: cloud::report(c, plan)?,
        publication: publication::report(c, plan)?,
    })
}

#[cfg(test)]
#[path = "migration_backup_tests.rs"]
mod tests;
