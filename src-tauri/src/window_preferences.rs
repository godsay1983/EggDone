use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::db::Database;

const KEY: &str = "main_window_preferences_v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WindowPreferences {
    width: f64,
    height: f64,
    zoom: f64,
}

impl WindowPreferences {
    fn validate(&self) -> Result<(), String> {
        if !self.width.is_finite()
            || !self.height.is_finite()
            || !(360.0..=4096.0).contains(&self.width)
            || !(420.0..=4096.0).contains(&self.height)
            || ![1.0, 1.15, 1.25, 1.5].contains(&self.zoom)
        {
            return Err("Invalid window preferences".into());
        }
        Ok(())
    }
}

fn read(connection: &Connection) -> Result<Option<WindowPreferences>, String> {
    let json: Option<String> = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    json.map(|json| {
        let prefs: WindowPreferences =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        prefs.validate()?;
        Ok(prefs)
    })
    .transpose()
}

fn write(connection: &Connection, prefs: &WindowPreferences) -> Result<(), String> {
    prefs.validate()?;
    let json = serde_json::to_string(prefs).map_err(|error| error.to_string())?;
    connection.execute(
        "INSERT INTO app_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, json],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Default)]
pub struct WindowPreferencesReady(pub std::sync::atomic::AtomicBool);

#[tauri::command]
pub fn get_window_preferences(
    database: State<'_, Database>,
) -> Result<Option<WindowPreferences>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|error| error.to_string())?;
    read(&connection)
}

#[tauri::command]
pub fn save_window_preferences(
    database: State<'_, Database>,
    ready: State<'_, WindowPreferencesReady>,
    preferences: WindowPreferences,
) -> Result<(), String> {
    let connection = database
        .connection
        .lock()
        .map_err(|error| error.to_string())?;
    write(&connection, &preferences)?;
    ready.0.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

// Native exit cannot wait for a debounced WebView callback. Preserve the latest
// actual size, but never save the default startup window before restoration.
pub fn flush_size(app: &tauri::AppHandle) -> Result<(), String> {
    if !app
        .state::<WindowPreferencesReady>()
        .0
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Ok(());
    }
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let database = app.state::<Database>();
    let connection = database
        .connection
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(mut prefs) = read(&connection)? {
        prefs.width = (size.width as f64 / scale).round().clamp(360.0, 4096.0);
        prefs.height = (size.height as f64 / scale).round().clamp(420.0, 4096.0);
        write(&connection, &prefs)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_database_reopen_and_rejects_invalid_updates() {
        let path =
            std::env::temp_dir().join(format!("eggdone-window-{}.sqlite", uuid::Uuid::new_v4()));
        let prefs = WindowPreferences {
            width: 700.0,
            height: 760.0,
            zoom: 1.25,
        };
        {
            let mut connection = Connection::open(&path).unwrap();
            crate::db::migrate(&mut connection).unwrap();
            assert_eq!(read(&connection).unwrap(), None);
            write(&connection, &prefs).unwrap();
            let invalid = WindowPreferences {
                zoom: 0.0,
                ..prefs.clone()
            };
            assert!(write(&connection, &invalid).is_err());
        }
        {
            let connection = Connection::open(&path).unwrap();
            assert_eq!(read(&connection).unwrap(), Some(prefs));
        }
        std::fs::remove_file(path).unwrap();
    }
}
