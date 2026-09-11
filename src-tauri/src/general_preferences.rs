use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::db::Database;

const KEY: &str = "general_preferences_v1";
const KEYS: &[&str] = &[
    "eggdone-theme",
    "eggdone-language",
    "eggdone-show-completed",
    "eggdone-default-list-view",
    "eggdone-list-view",
    "eggdone-selected-group",
    "eggdone-smart-view",
    "eggdone-focus-duration-minutes",
    "eggdone-break-duration-minutes",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeneralPreferences {
    version: u32,
    revision: u64,
    values: BTreeMap<String, Option<String>>,
}

impl GeneralPreferences {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.values.iter().any(|(key, value)| {
                !KEYS.contains(&key.as_str()) || value.as_ref().is_some_and(|v| v.len() > 256)
            })
        {
            return Err("Unsupported general preferences".into());
        }
        Ok(())
    }
}

fn read(connection: &Connection) -> Result<Option<GeneralPreferences>, String> {
    let json: Option<String> = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    json.map(|json| {
        let value: GeneralPreferences = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    })
    .transpose()
}

fn write(connection: &Connection, value: &GeneralPreferences) -> Result<(), String> {
    value.validate()?;
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    connection.execute(
        "INSERT INTO app_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, json],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn initialize(
    connection: &Connection,
    values: BTreeMap<String, Option<String>>,
) -> Result<GeneralPreferences, String> {
    // Another WebView may have migrated while this caller was reading legacy storage.
    if let Some(saved) = read(connection)? {
        return Ok(saved);
    }
    let saved = GeneralPreferences {
        version: 1,
        revision: 1,
        values,
    };
    write(connection, &saved)?;
    Ok(saved)
}

fn patch(
    connection: &Connection,
    key: String,
    value: Option<String>,
) -> Result<GeneralPreferences, String> {
    let mut saved = read(connection)?.ok_or("General preferences not initialized")?;
    saved.values.insert(key, value);
    saved.revision = saved
        .revision
        .checked_add(1)
        .ok_or("Preference revision overflow")?;
    write(connection, &saved)?;
    Ok(saved)
}

#[tauri::command]
pub fn get_general_preferences(
    database: State<'_, Database>,
) -> Result<Option<GeneralPreferences>, String> {
    let connection = database.connection.lock().map_err(|e| e.to_string())?;
    read(&connection)
}

#[tauri::command]
pub fn initialize_general_preferences(
    database: State<'_, Database>,
    values: BTreeMap<String, Option<String>>,
) -> Result<GeneralPreferences, String> {
    let connection = database.connection.lock().map_err(|e| e.to_string())?;
    initialize(&connection, values)
}

#[tauri::command]
pub fn patch_general_preference(
    app: tauri::AppHandle,
    database: State<'_, Database>,
    key: String,
    value: Option<String>,
) -> Result<GeneralPreferences, String> {
    let saved = {
        let connection = database.connection.lock().map_err(|e| e.to_string())?;
        patch(&connection, key, value)?
    };
    // Persistence has succeeded even if a closing WebView cannot receive the event.
    let _ = app.emit("general-preferences-changed", &saved);
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        db
    }

    #[test]
    fn migration_is_one_time_and_patch_preserves_other_windows() {
        let db = database();
        assert!(read(&db).unwrap().is_none());
        let legacy = BTreeMap::from([("eggdone-theme".into(), Some("dark".into()))]);
        initialize(&db, legacy).unwrap();
        initialize(&db, BTreeMap::new()).unwrap();
        patch(&db, "eggdone-language".into(), Some("en-US".into())).unwrap();
        let saved = patch(&db, "eggdone-show-completed".into(), Some("false".into())).unwrap();
        assert_eq!(saved.values["eggdone-theme"].as_deref(), Some("dark"));
        assert_eq!(saved.values["eggdone-language"].as_deref(), Some("en-US"));
        assert_eq!(saved.revision, 3);
        assert!(patch(&db, "secret".into(), Some("not-allowed".into())).is_err());
        assert_eq!(read(&db).unwrap().unwrap().revision, 3);
    }

    #[test]
    fn unreadable_or_future_data_is_never_overwritten() {
        let db = database();
        for json in ["broken", r#"{"version":2,"revision":1,"values":{}}"#] {
            db.execute(
                "INSERT OR REPLACE INTO app_metadata VALUES (?1, ?2)",
                params![KEY, json],
            )
            .unwrap();
            assert!(initialize(&db, BTreeMap::new()).is_err());
            assert!(patch(&db, "eggdone-theme".into(), None).is_err());
            let unchanged: String = db
                .query_row("SELECT value FROM app_metadata WHERE key=?1", [KEY], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(unchanged, json);
        }
    }

    #[test]
    fn old_database_other_metadata_and_read_only_failures_are_preserved() {
        let db = database();
        db.execute("INSERT INTO app_metadata VALUES ('existing', 'keep')", [])
            .unwrap();
        initialize(&db, BTreeMap::new()).unwrap();
        db.execute_batch("PRAGMA query_only=ON").unwrap();
        assert!(patch(&db, "eggdone-theme".into(), Some("light".into())).is_err());
        assert!(read(&db).unwrap().unwrap().values.is_empty());
        assert_eq!(
            db.query_row(
                "SELECT value FROM app_metadata WHERE key='existing'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "keep"
        );
    }
}
