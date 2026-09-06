use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Database;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShortcutKind {
    Panel,
    Note,
}

impl ShortcutKind {
    fn key(self) -> &'static str {
        match self {
            Self::Panel => "panel_shortcut_preferences_v1",
            Self::Note => "note_shortcut_preferences_v1",
        }
    }

    fn supports(self, shortcut: &str) -> bool {
        match self {
            Self::Panel => [
                "CommandOrControl+Shift+Space",
                "CommandOrControl+Alt+Space",
                "Alt+Shift+Space",
                "CommandOrControl+Shift+E",
            ]
            .contains(&shortcut),
            Self::Note => [
                "CommandOrControl+Shift+N",
                "CommandOrControl+Alt+N",
                "Alt+Shift+N",
            ]
            .contains(&shortcut),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ShortcutPreference {
    shortcut: String,
    enabled: bool,
}

fn read(connection: &Connection, kind: ShortcutKind) -> Result<Option<ShortcutPreference>, String> {
    let json: Option<String> = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [kind.key()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    json.map(|json| {
        let preference: ShortcutPreference =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if !kind.supports(&preference.shortcut) {
            return Err("Invalid shortcut preference".into());
        }
        Ok(preference)
    })
    .transpose()
}

fn write(
    connection: &Connection,
    kind: ShortcutKind,
    preference: &ShortcutPreference,
) -> Result<(), String> {
    if !kind.supports(&preference.shortcut) {
        return Err("Invalid shortcut preference".into());
    }
    let json = serde_json::to_string(preference).map_err(|error| error.to_string())?;
    connection.execute(
        "INSERT INTO app_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![kind.key(), json],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_shortcut_preference(
    database: State<'_, Database>,
    kind: ShortcutKind,
) -> Result<Option<ShortcutPreference>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|error| error.to_string())?;
    read(&connection, kind)
}

#[tauri::command]
pub fn save_shortcut_preference(
    database: State<'_, Database>,
    kind: ShortcutKind,
    preference: ShortcutPreference,
) -> Result<(), String> {
    let connection = database
        .connection
        .lock()
        .map_err(|error| error.to_string())?;
    write(&connection, kind, &preference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_both_shortcuts_across_reopen_without_changing_other_metadata() {
        let path =
            std::env::temp_dir().join(format!("eggdone-shortcut-{}.sqlite", uuid::Uuid::new_v4()));
        let note = ShortcutPreference {
            shortcut: "Alt+Shift+N".into(),
            enabled: true,
        };
        let panel = ShortcutPreference {
            shortcut: "Alt+Shift+Space".into(),
            enabled: false,
        };
        let identity;
        {
            let mut connection = Connection::open(&path).unwrap();
            crate::db::migrate(&mut connection).unwrap();
            identity = crate::db::device_id(&connection).unwrap();
            assert_eq!(read(&connection, ShortcutKind::Note).unwrap(), None);
            write(&connection, ShortcutKind::Note, &note).unwrap();
            write(&connection, ShortcutKind::Panel, &panel).unwrap();
            assert!(write(&connection, ShortcutKind::Note, &panel).is_err());
        }
        {
            let connection = Connection::open(&path).unwrap();
            assert_eq!(read(&connection, ShortcutKind::Note).unwrap(), Some(note));
            assert_eq!(read(&connection, ShortcutKind::Panel).unwrap(), Some(panel));
            assert_eq!(crate::db::device_id(&connection).unwrap(), identity);
        }
        std::fs::remove_file(path).unwrap();
    }
}
