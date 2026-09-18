//! Restore only into a newly created, disposable database. Never replaces the live store.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: String,
    client: String,
    schema: i64,
    tables: Vec<Table>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
}

fn invalid() -> String {
    "MIGRATION_RECOVERY_INVALID".into()
}

fn restore(c: &mut Connection, data: &[u8], plan: &BackupPlan) -> Result<(), String> {
    if data.len() > MAX_DATA || digest(data) != plan.data_hash {
        return Err(invalid());
    }
    let snapshot: Snapshot = serde_json::from_slice(data).map_err(|_| invalid())?;
    if snapshot.format != "eggdone.local-migration-recovery.v1"
        || snapshot.client != "desktop"
        || snapshot.schema != 22
        || snapshot.tables.len() != TABLES.len()
    {
        return Err(invalid());
    }
    // Refuse any existing database, even if its business tables look empty.
    let existing: i64 = c
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
        .map_err(db)?;
    if existing != 0 {
        return Err("MIGRATION_RECOVERY_TARGET".into());
    }
    crate::db::configure_connection(c).map_err(db)?;
    crate::db::migrate(c).map_err(db)?;
    let tx = c.transaction().map_err(db)?;
    for (table, expected) in snapshot.tables.iter().zip(TABLES.iter()) {
        if table.name != *expected || table.rows.len() > 100_000 {
            return Err(invalid());
        }
        let statement = tx
            .prepare(&format!("SELECT * FROM {expected} LIMIT 0"))
            .map_err(db)?;
        if table.columns != statement.column_names() {
            return Err(invalid());
        }
        for row in &table.rows {
            if row.len() != table.columns.len()
                || row.iter().any(|v| match v {
                    serde_json::Value::Null | serde_json::Value::String(_) => false,
                    serde_json::Value::Number(n) => !n.as_i64().is_some_and(|n| {
                        (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n)
                    }),
                    _ => true,
                })
            {
                return Err(invalid());
            }
        }
    }
    // Clear only fresh migration seeds. Keep triggers enabled, load guards before business rows,
    // and restore sync counters after business inserts so no artificial dirty revisions survive.
    for table in TABLES.iter().rev() {
        tx.execute(&format!("DELETE FROM {table}"), [])
            .map_err(db)?;
    }
    let order = ["lifecycle_terminals", "purge_cleanup"].into_iter().chain(
        TABLES
            .iter()
            .copied()
            .filter(|t| !["lifecycle_terminals", "purge_cleanup"].contains(t)),
    );
    for name in order {
        let index = TABLES.iter().position(|t| *t == name).ok_or_else(invalid)?;
        let table = &snapshot.tables[index];
        // Names/columns were matched against the newly migrated schema, never taken as arbitrary SQL.
        let sql = format!(
            "INSERT INTO {name} ({}) VALUES ({})",
            table.columns.join(","),
            vec!["?"; table.columns.len()].join(",")
        );
        let mut statement = tx.prepare(&sql).map_err(db)?;
        for row in &table.rows {
            let values: Vec<rusqlite::types::Value> = row
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                    serde_json::Value::Number(n) => {
                        rusqlite::types::Value::Integer(n.as_i64().unwrap_or_default())
                    }
                    _ => rusqlite::types::Value::Null,
                })
                .collect();
            statement
                .execute(rusqlite::params_from_iter(values))
                .map_err(db)?;
        }
    }
    let foreign_key_error = tx
        .prepare("PRAGMA foreign_key_check")
        .map_err(db)?
        .exists([])
        .map_err(db)?;
    if foreign_key_error
        || digest(&capture(&tx)?) != plan.data_hash
        || assets(&tx)? != plan.files[1..]
    {
        return Err(invalid());
    }
    tx.commit().map_err(db)
}

pub fn rehearse(root: &Path, plan: &BackupPlan) -> Result<(), String> {
    verify_files(root, plan)?;
    let backup = folder(root, plan, false)?;
    let mut data = Vec::new();
    fs::File::open(backup.join("data.json"))
        .map_err(io)?
        .take(MAX_DATA as u64 + 1)
        .read_to_end(&mut data)
        .map_err(io)?;
    if data.len() > MAX_DATA || digest(&data) != plan.data_hash {
        return Err(invalid());
    }
    let scratch = backup.join(format!("recovery-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&scratch).map_err(io)?;
    let result = (|| {
        directory(&scratch, true)?;
        let path = scratch.join("recovery.db");
        let mut c = Connection::open(&path).map_err(db)?;
        restore(&mut c, &data, plan)?;
        c.close().map_err(|(_, e)| db(e))?;
        let c = Connection::open(&path).map_err(db)?;
        let integrity: String = c
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(db)?;
        if integrity != "ok" || digest(&capture(&c)?) != plan.data_hash {
            return Err(invalid());
        }
        c.close().map_err(|(_, e)| db(e))?;
        let restored_assets = scratch.join("note-assets");
        fs::create_dir(&restored_assets).map_err(io)?;
        for entry in plan.files.iter().skip(1) {
            let parent = restored_assets.join(&entry.name[..36]);
            directory(&parent, true)?;
            let output = parent.join(&entry.name[37..]);
            fs::copy(backup.join(&entry.name), &output).map_err(io)?;
            fs::OpenOptions::new()
                .write(true)
                .open(&output)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
            verify(&output, entry)?;
        }
        Ok(())
    })();
    // The path was generated here and must remain a direct child of this verified backup.
    let owned = fs::canonicalize(&scratch).map_err(io)?;
    if owned.parent() != Some(fs::canonicalize(&backup).map_err(io)?.as_path()) {
        return Err(invalid());
    }
    fs::remove_dir_all(&owned).map_err(|_| "MIGRATION_RECOVERY_CLEANUP")?;
    result.map_err(|_: String| "MIGRATION_RECOVERY_FAILED".into())
}

#[cfg(test)]
#[path = "migration_recovery_tests.rs"]
mod tests;
