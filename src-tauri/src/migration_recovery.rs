//! Restore only into a newly created, disposable database. Never replaces the live store.
use super::*;

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: String,
    client: String,
    schema: i64,
    tables: Vec<Table>,
}
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
}

fn invalid() -> String {
    "MIGRATION_RECOVERY_INVALID".into()
}

fn column_mapping(table: &Table, actual: &[&str], schema: i64) -> Result<Vec<usize>, String> {
    let expected: std::collections::BTreeSet<_> =
        table.columns.iter().map(String::as_str).collect();
    let present: std::collections::BTreeSet<_> = actual.iter().copied().collect();
    let mut supported = expected.clone();
    if schema == 22 && table.name == "purge_cleanup" {
        supported.insert("local_attempts");
    }
    if expected.len() != table.columns.len() || supported != present {
        return Err("MIGRATION_RECOVERY_COLUMNS".into());
    }
    table
        .columns
        .iter()
        .map(|name| {
            actual
                .iter()
                .position(|c| *c == name)
                .ok_or_else(|| "MIGRATION_RECOVERY_COLUMNS".into())
        })
        .collect()
}

fn verify_restored(c: &Connection, data: &[u8]) -> Result<(), String> {
    let expected: Snapshot = serde_json::from_slice(data).map_err(|_| invalid())?;
    let mut actual: Snapshot = serde_json::from_slice(&capture(c)?).map_err(|_| invalid())?;
    if expected.schema < 24 {
        actual
            .tables
            .truncate(tables_for_schema(expected.schema).len());
    }
    if actual.tables.len() != expected.tables.len() {
        return Err(invalid());
    }
    for (table, source) in actual.tables.iter_mut().zip(&expected.tables) {
        if table.name != source.name {
            return Err(invalid());
        }
        let columns = table.columns.iter().map(String::as_str).collect::<Vec<_>>();
        let mapping = column_mapping(source, &columns, expected.schema)?;
        // Only the known v22 omission can be defaulted; all source values remain exact.
        if table.name == "purge_cleanup" && !source.columns.iter().any(|c| c == "local_attempts") {
            let index = columns
                .iter()
                .position(|c| *c == "local_attempts")
                .ok_or_else(invalid)?;
            if table.rows.iter().any(|r| r[index] != serde_json::json!(0)) {
                return Err(invalid());
            }
        }
        table.rows = table
            .rows
            .iter()
            .map(|r| mapping.iter().map(|i| r[*i].clone()).collect())
            .collect();
        table.columns = source.columns.clone();
    }
    actual.schema = expected.schema;
    if actual != expected {
        return Err("MIGRATION_RECOVERY_CONTENT".into());
    }
    Ok(())
}

fn restore(c: &mut Connection, data: &[u8], plan: &BackupPlan) -> Result<(), String> {
    if data.len() > MAX_DATA || digest(data) != plan.data_hash {
        return Err(invalid());
    }
    let snapshot: Snapshot = serde_json::from_slice(data).map_err(|_| invalid())?;
    if snapshot.format != "eggdone.local-migration-recovery.v1"
        || snapshot.client != "desktop"
        || ![22, 23, 24, 25].contains(&snapshot.schema)
        || snapshot.tables.len() != tables_for_schema(snapshot.schema).len()
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
    let source_tables = tables_for_schema(snapshot.schema);
    for (table, expected) in snapshot.tables.iter().zip(source_tables.iter()) {
        if table.name != *expected || table.rows.len() > 100_000 {
            return Err(invalid());
        }
        let statement = tx
            .prepare(&format!("SELECT * FROM {expected} LIMIT 0"))
            .map_err(db)?;
        column_mapping(table, &statement.column_names(), snapshot.schema)?;
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
    for table in source_tables.iter().rev() {
        tx.execute(&format!("DELETE FROM {table}"), [])
            .map_err(db)?;
    }
    let order = ["lifecycle_terminals", "purge_cleanup"].into_iter().chain(
        source_tables
            .iter()
            .copied()
            .filter(|t| !["lifecycle_terminals", "purge_cleanup"].contains(t)),
    );
    for name in order {
        let index = source_tables
            .iter()
            .position(|t| *t == name)
            .ok_or_else(invalid)?;
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
                .map_err(|_| format!("MIGRATION_RECOVERY_INSERT_{}", name.to_ascii_uppercase()))?;
        }
    }
    let foreign_key_error = tx
        .prepare("PRAGMA foreign_key_check")
        .map_err(db)?
        .exists([])
        .map_err(db)?;
    if foreign_key_error {
        return Err("MIGRATION_RECOVERY_REFERENCES".into());
    }
    verify_restored(&tx, data)?;
    if assets(&tx)? != identities(&plan.files[1..]) {
        return Err("MIGRATION_RECOVERY_ASSETS".into());
    }
    for file in plan.files.iter().filter(|f| f.missing) {
        if !crate::migration_backup::can_omit(&tx, file)? {
            return Err(invalid());
        }
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
        if integrity != "ok" {
            return Err(invalid());
        }
        verify_restored(&c, &data)?;
        c.close().map_err(|(_, e)| db(e))?;
        let restored_assets = scratch.join("note-assets");
        fs::create_dir(&restored_assets).map_err(io)?;
        for entry in plan.files.iter().skip(1).filter(|f| !f.missing) {
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
    result.map_err(|error: String| {
        if error.starts_with("MIGRATION_RECOVERY_") {
            error
        } else {
            "MIGRATION_RECOVERY_FAILED".into()
        }
    })
}

#[cfg(test)]
#[path = "migration_recovery_tests.rs"]
mod tests;
