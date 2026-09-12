use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, Transaction};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: i64 = 18;
const DEVICE_ID_KEY: &str = "device_id";

pub struct Database {
    pub(crate) connection: Mutex<Connection>,
}

impl Database {
    pub fn open(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_path(database_path(app)?)
    }

    fn open_path(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut connection = Connection::open(path)?;
        configure_connection(&connection)?;
        migrate(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub(crate) fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        ",
    )
}

pub(crate) fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );
        ",
    )?;

    if schema_version(connection)? > CURRENT_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }

    apply_migration(connection, 1, create_legacy_schema)?;
    apply_migration(connection, 2, migrate_todos_for_sync)?;
    apply_migration(connection, 3, add_sync_identity)?;
    apply_migration(connection, 4, add_sync_settings)?;
    apply_migration(connection, 5, add_todo_pinning)?;
    apply_migration(connection, 6, add_todo_due_fields)?;
    apply_migration(connection, 7, add_reminder_deliveries)?;
    apply_migration(connection, 8, add_todo_groups)?;
    apply_migration(connection, 9, add_todo_repeats)?;
    apply_migration(connection, 10, add_repeat_series_uuid)?;
    apply_migration(connection, 11, add_todo_note)?;
    apply_migration(connection, 12, add_todo_archive)?;
    apply_migration(connection, 13, add_todo_priority)?;
    apply_migration(connection, 14, add_notes)?;
    apply_migration(connection, 15, add_note_attachments)?;
    apply_migration(connection, 16, add_sync_runtime_state)?;
    apply_migration(connection, 17, |transaction| {
        transaction.execute_batch(include_str!("migrations/017_recurrence_rules.sql"))
    })?;
    apply_migration(connection, 18, |transaction| {
        transaction.execute_batch(include_str!("migrations/018_task_note_links.sql"))
    })?;

    debug_assert_eq!(schema_version(connection)?, CURRENT_SCHEMA_VERSION);
    Ok(())
}

fn apply_migration(
    connection: &mut Connection,
    version: i64,
    migration: fn(&Transaction<'_>) -> rusqlite::Result<()>,
) -> rusqlite::Result<()> {
    if migration_applied(connection, version)? {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    migration(&transaction)?;
    transaction.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
        params![version, now_millis()],
    )?;
    transaction.commit()
}

fn migration_applied(connection: &Connection, version: i64) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
        params![version],
        |row| row.get(0),
    )
}

fn schema_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )
}

fn create_legacy_schema(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL CHECK(length(trim(title)) > 0),
            completed INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        ",
    )
}

#[derive(Debug)]
struct LegacyTodo {
    id: i64,
    title: String,
    completed: bool,
    created_at: i64,
    updated_at: i64,
}

fn migrate_todos_for_sync(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    let legacy_todos = {
        let mut statement = transaction.prepare(
            "
            SELECT
                id,
                title,
                completed,
                CASE
                    WHEN typeof(created_at) = 'integer' THEN created_at
                    ELSE CAST((julianday(created_at) - 2440587.5) * 86400000 AS INTEGER)
                END,
                CASE
                    WHEN typeof(updated_at) = 'integer' THEN updated_at
                    ELSE CAST((julianday(updated_at) - 2440587.5) * 86400000 AS INTEGER)
                END
            FROM todos
            ORDER BY completed ASC, created_at DESC, id DESC
            ",
        )?;

        let todos = statement
            .query_map([], |row| {
                Ok(LegacyTodo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    completed: row.get::<_, i64>(2)? != 0,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        todos
    };

    transaction.execute_batch(
        "
        ALTER TABLE todos RENAME TO todos_legacy;

        CREATE TABLE todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL CHECK(length(trim(title)) > 0),
            completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
            sort_order INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            completed_at INTEGER,
            deleted_at INTEGER
        );

        CREATE INDEX idx_todos_active_order
            ON todos(deleted_at, completed, sort_order);
        CREATE INDEX idx_todos_updated_at ON todos(updated_at);
        ",
    )?;

    for (index, todo) in legacy_todos.into_iter().enumerate() {
        let completed_at = todo.completed.then_some(todo.updated_at);
        transaction.execute(
            "
            INSERT INTO todos (
                id, uuid, title, completed, sort_order,
                created_at, updated_at, completed_at, deleted_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            ",
            params![
                todo.id,
                Uuid::new_v4().to_string(),
                todo.title,
                todo.completed,
                index as i64 * 1024,
                todo.created_at,
                todo.updated_at,
                completed_at,
            ],
        )?;
    }

    transaction.execute_batch("DROP TABLE todos_legacy;")
}

fn add_sync_identity(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE app_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        ALTER TABLE todos
            ADD COLUMN updated_by TEXT NOT NULL DEFAULT '';
        ",
    )?;
    let device_id = Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO app_metadata (key, value) VALUES (?1, ?2)",
        params![DEVICE_ID_KEY, device_id],
    )?;
    transaction.execute(
        "UPDATE todos SET updated_by = ?1 WHERE updated_by = ''",
        params![device_id],
    )?;
    Ok(())
}

fn add_sync_settings(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute(
        "
        CREATE TABLE sync_settings (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
            endpoint TEXT NOT NULL DEFAULT '',
            region TEXT NOT NULL DEFAULT 'us-east-1',
            bucket TEXT NOT NULL DEFAULT '',
            object_key TEXT NOT NULL DEFAULT 'eggdone/todos.json',
            path_style INTEGER NOT NULL DEFAULT 1 CHECK(path_style IN (0, 1)),
            allow_http INTEGER NOT NULL DEFAULT 0 CHECK(allow_http IN (0, 1)),
            updated_at INTEGER NOT NULL
        )
        ",
        [],
    )?;
    transaction.execute(
        "
        INSERT INTO sync_settings (
            id, enabled, endpoint, region, bucket, object_key,
            path_style, allow_http, updated_at
        )
        VALUES (1, 0, '', 'us-east-1', '', 'eggdone/todos.json', 1, 0, ?1)
        ",
        params![now_millis()],
    )?;
    Ok(())
}

fn add_todo_pinning(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0, 1));

        DROP INDEX idx_todos_active_order;
        CREATE INDEX idx_todos_active_order
            ON todos(deleted_at, completed, pinned DESC, sort_order);
        ",
    )
}

fn add_todo_due_fields(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN due_date TEXT CHECK(due_date IS NULL OR length(due_date) = 10);
        ALTER TABLE todos
            ADD COLUMN due_at INTEGER;
        ALTER TABLE todos
            ADD COLUMN reminder_at INTEGER;

        CREATE INDEX idx_todos_due_date
            ON todos(deleted_at, completed, due_date);
        CREATE INDEX idx_todos_reminder_at
            ON todos(deleted_at, completed, reminder_at);
        ",
    )
}

fn add_reminder_deliveries(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS reminder_deliveries (
            todo_uuid TEXT NOT NULL,
            device_id TEXT NOT NULL,
            reminder_at INTEGER NOT NULL,
            fired_at INTEGER NOT NULL,
            PRIMARY KEY (todo_uuid, device_id, reminder_at)
        );
        ",
    )
}

fn add_todo_groups(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL CHECK(length(trim(name)) > 0),
            color TEXT NOT NULL DEFAULT 'yellow',
            sort_order INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            updated_by TEXT NOT NULL
        );

        CREATE INDEX idx_groups_active_order
            ON groups(deleted_at, sort_order);
        CREATE INDEX idx_groups_updated_at ON groups(updated_at);

        ALTER TABLE todos
            ADD COLUMN group_uuid TEXT;
        CREATE INDEX idx_todos_group_uuid
            ON todos(group_uuid, deleted_at, completed, pinned DESC, sort_order);
        ",
    )
}

fn add_todo_repeats(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN repeat_rule TEXT
                CHECK(repeat_rule IS NULL OR repeat_rule IN ('daily', 'weekly', 'monthly', 'weekdays'));
        ALTER TABLE todos
            ADD COLUMN repeat_next_due_date TEXT
                CHECK(repeat_next_due_date IS NULL OR length(repeat_next_due_date) = 10);

        CREATE INDEX idx_todos_repeat_next_due_date
            ON todos(deleted_at, repeat_rule, repeat_next_due_date);
        ",
    )
}

fn add_repeat_series_uuid(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN repeat_series_uuid TEXT;

        UPDATE todos
        SET repeat_series_uuid = uuid
        WHERE repeat_rule IS NOT NULL AND repeat_series_uuid IS NULL;

        CREATE INDEX idx_todos_repeat_series_uuid
            ON todos(repeat_series_uuid, deleted_at);
        ",
    )
}

fn add_todo_note(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN note TEXT;
        ",
    )
}

fn add_todo_archive(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN archived_at INTEGER;
        CREATE INDEX idx_todos_archived_at
            ON todos(archived_at, deleted_at, completed);
        ",
    )
}

fn add_todo_priority(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        ALTER TABLE todos
            ADD COLUMN priority INTEGER NOT NULL DEFAULT 0 CHECK(priority IN (0, 1));
        ",
    )
}

fn add_notes(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '' CHECK(length(title) <= 100),
            content TEXT NOT NULL DEFAULT '' CHECK(length(content) <= 20000),
            color TEXT NOT NULL DEFAULT 'default'
                CHECK(color IN ('default', 'yellow', 'pink', 'green', 'blue')),
            pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0, 1)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            updated_by TEXT NOT NULL,
            CHECK(length(trim(title)) > 0 OR length(trim(content)) > 0)
        );

        CREATE INDEX idx_notes_active_order
            ON notes(deleted_at, pinned DESC, updated_at DESC);
        CREATE INDEX idx_notes_updated_at ON notes(updated_at);
        ",
    )
}

fn add_note_attachments(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS note_attachments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT NOT NULL UNIQUE,
            note_uuid TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('image', 'file')),
            display_name TEXT NOT NULL CHECK(length(trim(display_name)) > 0),
            mime_type TEXT NOT NULL,
            byte_size INTEGER NOT NULL CHECK(byte_size > 0),
            sha256 TEXT NOT NULL,
            preview_mime_type TEXT,
            preview_byte_size INTEGER,
            preview_sha256 TEXT,
            width INTEGER,
            height INTEGER,
            sort_order INTEGER NOT NULL DEFAULT 0 CHECK(sort_order >= 0),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            updated_by TEXT NOT NULL,
            local_original_path TEXT,
            local_preview_path TEXT,
            transfer_state TEXT NOT NULL DEFAULT 'pending_upload'
                CHECK(transfer_state IN (
                    'pending_upload', 'uploading', 'uploaded', 'synced',
                    'remote_only', 'downloading', 'cached', 'failed'
                )),
            transfer_error TEXT,
            remote_uploaded INTEGER NOT NULL DEFAULT 0 CHECK(remote_uploaded IN (0, 1)),
            CHECK(
                (kind = 'image'
                    AND preview_mime_type = 'image/jpeg'
                    AND preview_byte_size IS NOT NULL
                    AND preview_sha256 IS NOT NULL
                    AND width IS NOT NULL
                    AND height IS NOT NULL)
                OR
                (kind = 'file'
                    AND preview_mime_type IS NULL
                    AND preview_byte_size IS NULL
                    AND preview_sha256 IS NULL
                    AND width IS NULL
                    AND height IS NULL)
            )
        );

        CREATE INDEX IF NOT EXISTS idx_note_attachments_note_order
            ON note_attachments(note_uuid, deleted_at, sort_order, uuid);
        CREATE INDEX IF NOT EXISTS idx_note_attachments_transfer
            ON note_attachments(transfer_state, updated_at);

        CREATE TRIGGER IF NOT EXISTS notes_soft_delete_attachments
        AFTER UPDATE OF deleted_at ON notes
        WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
        BEGIN
            UPDATE note_attachments
            SET deleted_at = NEW.deleted_at,
                updated_at = NEW.updated_at,
                updated_by = NEW.updated_by
            WHERE note_uuid = NEW.uuid AND deleted_at IS NULL;
        END;

        CREATE TRIGGER IF NOT EXISTS notes_restore_attachments
        AFTER UPDATE OF deleted_at ON notes
        WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
        BEGIN
            UPDATE note_attachments
            SET deleted_at = NULL,
                updated_at = NEW.updated_at,
                updated_by = NEW.updated_by
            WHERE note_uuid = NEW.uuid AND deleted_at = OLD.deleted_at;
        END;
        ",
    )
}

fn add_sync_runtime_state(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute_batch(
        "
        CREATE TABLE sync_runtime_state (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            schema_version INTEGER NOT NULL DEFAULT 1,
            last_attempt_at INTEGER,
            last_success_at INTEGER,
            dirty_since INTEGER,
            dirty_domains TEXT NOT NULL DEFAULT '[]',
            last_result TEXT NOT NULL DEFAULT 'never'
                CHECK(last_result IN ('never', 'success', 'offline', 'conflict', 'failed', 'interrupted')),
            last_error_code TEXT,
            last_error_message TEXT,
            pending_attachment_count INTEGER NOT NULL DEFAULT 0 CHECK(pending_attachment_count >= 0),
            todos_dirty_version INTEGER NOT NULL DEFAULT 0,
            notes_dirty_version INTEGER NOT NULL DEFAULT 0,
            attachments_dirty_version INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        );

        INSERT INTO sync_runtime_state (id, updated_at) VALUES (1, 0);
        ",
    )?;

    for (table, event, domain, update_columns) in [
        ("todos", "insert", "todos", ""),
        ("todos", "update", "todos", ""),
        ("todos", "delete", "todos", ""),
        ("groups", "insert", "todos", ""),
        ("groups", "update", "todos", ""),
        ("groups", "delete", "todos", ""),
        ("notes", "insert", "notes", ""),
        ("notes", "update", "notes", ""),
        ("notes", "delete", "notes", ""),
        ("note_attachments", "insert", "attachments", ""),
        (
            "note_attachments",
            "update",
            "attachments",
            " OF note_uuid, kind, display_name, mime_type, byte_size, sha256, preview_mime_type, preview_byte_size, preview_sha256, width, height, sort_order, created_at, updated_at, deleted_at, updated_by",
        ),
        ("note_attachments", "delete", "attachments", ""),
    ] {
        create_sync_dirty_trigger(transaction, table, event, domain, update_columns)?;
    }
    Ok(())
}

fn create_sync_dirty_trigger(
    transaction: &Transaction<'_>,
    table: &str,
    event: &str,
    domain: &str,
    update_columns: &str,
) -> rusqlite::Result<()> {
    let domain_expression = match domain {
        "todos" => "CASE
            WHEN instr(dirty_domains, '\"todos\"') > 0 THEN dirty_domains
            WHEN dirty_domains = '[]' THEN '[\"todos\"]'
            ELSE '[\"todos\",' || substr(dirty_domains, 2)
        END",
        "notes" => "CASE
            WHEN instr(dirty_domains, '\"notes\"') > 0 THEN dirty_domains
            WHEN dirty_domains = '[]' THEN '[\"notes\"]'
            WHEN instr(dirty_domains, '\"todos\"') > 0 AND instr(dirty_domains, '\"attachments\"') > 0
                THEN '[\"todos\",\"notes\",\"attachments\"]'
            WHEN instr(dirty_domains, '\"todos\"') > 0 THEN '[\"todos\",\"notes\"]'
            ELSE '[\"notes\",\"attachments\"]'
        END",
        "attachments" => "CASE
            WHEN instr(dirty_domains, '\"attachments\"') > 0 THEN dirty_domains
            WHEN dirty_domains = '[]' THEN '[\"attachments\"]'
            ELSE substr(dirty_domains, 1, length(dirty_domains) - 1) || ',\"attachments\"]'
        END",
        _ => unreachable!(),
    };
    let timestamp = "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)";
    let version_column = match domain {
        "todos" => "todos_dirty_version",
        "notes" => "notes_dirty_version",
        "attachments" => "attachments_dirty_version",
        _ => unreachable!(),
    };
    let sql = format!(
        "CREATE TRIGGER sync_dirty_{table}_{event}
         AFTER {event}{update_columns} ON {table}
         BEGIN
           UPDATE sync_runtime_state
           SET dirty_since = COALESCE(dirty_since, {timestamp}),
               dirty_domains = {domain_expression},
               pending_attachment_count = (
                 SELECT COUNT(*) FROM note_attachments
                 WHERE deleted_at IS NULL
                   AND transfer_state IN ('pending_upload', 'uploading', 'uploaded', 'failed')
                   AND NOT (transfer_state = 'failed' AND remote_uploaded = 1)
               ),
               {version_column} = {version_column} + 1,
               updated_at = {timestamp}
           WHERE id = 1;
         END;"
    );
    transaction.execute_batch(&sql)
}

pub(crate) fn device_id(connection: &Connection) -> rusqlite::Result<String> {
    connection.query_row(
        "SELECT value FROM app_metadata WHERE key = ?1",
        params![DEVICE_ID_KEY],
        |row| row.get(0),
    )
}

fn database_path(app: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Use the platform-specific app data directory on Windows, macOS, and Linux.
    let directory = app.path().app_data_dir()?;
    fs::create_dir_all(&directory)?;
    Ok(directory.join("eggdone.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove_sync_runtime_migration(connection: &Connection) {
        for table in ["todos", "groups", "notes", "note_attachments"] {
            for event in ["insert", "update", "delete"] {
                connection
                    .execute_batch(&format!(
                        "DROP TRIGGER IF EXISTS sync_dirty_{table}_{event};"
                    ))
                    .unwrap();
            }
        }
        connection
            .execute_batch(
                "DROP TABLE IF EXISTS sync_runtime_state;
                 DELETE FROM schema_migrations WHERE version = 16;",
            )
            .unwrap();
    }

    fn open_memory_database() -> Database {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        Database {
            connection: Mutex::new(connection),
        }
    }

    #[test]
    fn creates_current_schema_for_new_database() {
        let database = open_memory_database();
        let connection = database.connection.lock().unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(todos)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        for expected in [
            "uuid",
            "completed_at",
            "deleted_at",
            "sort_order",
            "created_at",
            "updated_at",
            "updated_by",
            "pinned",
            "due_date",
            "due_at",
            "reminder_at",
            "group_uuid",
            "repeat_rule",
            "repeat_next_due_date",
            "repeat_series_uuid",
            "note",
            "archived_at",
            "priority",
        ] {
            assert!(columns.iter().any(|column| column == expected));
        }

        let sync_settings_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync_settings_count, 1);
        let reminder_delivery_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM reminder_deliveries", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(reminder_delivery_count, 0);
        let group_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM groups", [], |row| row.get(0))
            .unwrap();
        assert_eq!(group_count, 0);
        let note_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(note_count, 0);
        let attachment_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM note_attachments", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(attachment_count, 0);
        let runtime_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_runtime_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(runtime_count, 1);
    }

    #[test]
    fn sync_runtime_triggers_mark_domains_and_versions_dirty() {
        let database = open_memory_database();
        let connection = database.connection.lock().unwrap();
        let identity = device_id(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO todos (
                    uuid, title, completed, sort_order, created_at, updated_at, updated_by
                 ) VALUES (?1, 'dirty todo', 0, 0, 1, 1, ?2)",
                params!["00000000-0000-4000-8000-000000000016", identity],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO notes (
                    uuid, title, content, color, pinned, created_at, updated_at, updated_by
                 ) VALUES (?1, 'dirty note', '', 'default', 0, 1, 1, ?2)",
                params!["00000000-0000-4000-8000-000000000017", identity],
            )
            .unwrap();
        let domains: String = connection
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(domains, "[\"todos\",\"notes\"]");
        let versions: (i64, i64, i64) = connection
            .query_row(
                "SELECT todos_dirty_version, notes_dirty_version, attachments_dirty_version
                 FROM sync_runtime_state",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(versions, (1, 1, 0));
        assert!(connection
            .query_row("SELECT dirty_since FROM sync_runtime_state", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .unwrap()
            .is_some());
    }

    #[test]
    fn sync_runtime_dirty_state_survives_database_reopen() {
        let path =
            std::env::temp_dir().join(format!("eggdone-sync-runtime-{}.sqlite3", Uuid::new_v4()));
        {
            let database = Database::open_path(&path).unwrap();
            let connection = database.connection.lock().unwrap();
            let identity = device_id(&connection).unwrap();
            connection
                .execute(
                    "INSERT INTO todos (
                        uuid, title, completed, sort_order, created_at, updated_at, updated_by
                     ) VALUES (?1, 'persist dirty state', 0, 0, 1, 1, ?2)",
                    params![Uuid::new_v4().to_string(), identity],
                )
                .unwrap();
        }
        {
            let database = Database::open_path(&path).unwrap();
            let connection = database.connection.lock().unwrap();
            let domains: String = connection
                .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(domains, "[\"todos\"]");
        }
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}-wal", path.display()));
        let _ = fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn upgrades_legacy_database_without_losing_rows() {
        let mut connection = Connection::open_in_memory().unwrap();
        let transaction = connection.transaction().unwrap();
        create_legacy_schema(&transaction).unwrap();
        transaction.commit().unwrap();
        connection
            .execute(
                "
                INSERT INTO todos (title, completed, created_at, updated_at)
                VALUES ('旧任务', 1, '2026-06-06 12:00:00', '2026-06-06 13:00:00')
                ",
                [],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let todo = connection
            .query_row(
                "
                SELECT title, completed, created_at, updated_at, completed_at, deleted_at, uuid
                FROM todos
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(todo.0, "旧任务");
        assert_eq!(todo.1, 1);
        assert!(todo.2 > 0);
        assert!(todo.3 >= todo.2);
        assert_eq!(todo.4, Some(todo.3));
        assert_eq!(todo.5, None);
        assert!(Uuid::parse_str(&todo.6).is_ok());
        assert!(Uuid::parse_str(&device_id(&connection).unwrap()).is_ok());
        let updated_by: String = connection
            .query_row("SELECT updated_by FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(updated_by, device_id(&connection).unwrap());
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();

        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let migration_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(migration_count, CURRENT_SCHEMA_VERSION);
        let first_device_id = device_id(&connection).unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(device_id(&connection).unwrap(), first_device_id);
    }

    #[test]
    fn upgrades_v13_database_with_notes_without_touching_todos() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        remove_sync_runtime_migration(&connection);
        connection
            .execute_batch(
                "
            DROP TABLE note_attachments;
            DROP TABLE notes;
            DELETE FROM schema_migrations WHERE version >= 14;
            ",
            )
            .unwrap();
        let identity = device_id(&connection).unwrap();
        connection
            .execute(
                "
            INSERT INTO todos (
                uuid, title, completed, sort_order, created_at, updated_at, updated_by
            )
            VALUES (?1, '保留任务', 0, 0, 100, 100, ?2)
            ",
                params!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", identity],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let title: String = connection
            .query_row("SELECT title FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "保留任务");
        let note_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'notes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(note_table_count, 1);
    }

    #[test]
    fn upgrades_v14_database_with_attachments_without_touching_notes() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        remove_sync_runtime_migration(&connection);
        connection
            .execute_batch(
                "
                DROP TABLE note_attachments;
                DELETE FROM schema_migrations WHERE version = 15;
                ",
            )
            .unwrap();
        let identity = device_id(&connection).unwrap();
        connection
            .execute(
                "
                INSERT INTO notes (
                    uuid, title, content, color, pinned,
                    created_at, updated_at, deleted_at, updated_by
                ) VALUES (?1, '保留便签', '正文', 'yellow', 0, 100, 100, NULL, ?2)
                ",
                params!["cccccccc-cccc-4ccc-8ccc-cccccccccccc", identity],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let title: String = connection
            .query_row("SELECT title FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "保留便签");
        let attachment_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'note_attachments'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attachment_table_count, 1);
    }

    #[test]
    fn upgrades_v2_database_with_stable_sync_identity() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        connection
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, sort_order, created_at, updated_at,
                    completed_at, deleted_at
                )
                VALUES (?1, 'v2 task', 0, 0, 1, 2, NULL, NULL)
                ",
                params!["00000000-0000-4000-8000-000000000001"],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let identity = device_id(&connection).unwrap();
        let updated_by: String = connection
            .query_row("SELECT updated_by FROM todos", [], |row| row.get(0))
            .unwrap();
        assert!(Uuid::parse_str(&identity).is_ok());
        assert_eq!(updated_by, identity);
    }

    #[test]
    fn upgrades_v3_database_with_default_sync_settings() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();

        migrate(&mut connection).unwrap();

        let settings = connection
            .query_row(
                "
                SELECT enabled, endpoint, region, bucket, object_key, path_style, allow_http
                FROM sync_settings WHERE id = 1
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(
            settings,
            (
                0,
                String::new(),
                "us-east-1".to_string(),
                String::new(),
                "eggdone/todos.json".to_string(),
                1,
                0,
            )
        );
    }

    #[test]
    fn upgrades_v4_database_with_unpinned_todos() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        connection
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, sort_order, created_at, updated_at,
                    completed_at, deleted_at, updated_by
                )
                VALUES (?1, 'v4 task', 0, 0, 1, 2, NULL, NULL, ?2)
                ",
                params![
                    "00000000-0000-4000-8000-000000000001",
                    device_id(&connection).unwrap()
                ],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let pinned: i64 = connection
            .query_row("SELECT pinned FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pinned, 0);
    }

    #[test]
    fn upgrades_v5_database_with_empty_due_fields() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();

        migrate(&mut connection).unwrap();

        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(todos)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "due_date"));
        assert!(columns.iter().any(|column| column == "due_at"));
        assert!(columns.iter().any(|column| column == "reminder_at"));

        let reminder_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'reminder_deliveries'",
                [],
                |row| row.get(0),
        )
        .unwrap();
        assert_eq!(reminder_tables, 1);
    }

    #[test]
    fn upgrades_v6_database_with_missing_reminder_deliveries() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), 6);
        let missing_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'reminder_deliveries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(missing_table_count, 0);

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let reminder_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'reminder_deliveries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reminder_tables, 1);
    }

    #[test]
    fn upgrades_v7_database_with_group_support() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();
        apply_migration(&mut connection, 7, add_reminder_deliveries).unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(todos)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "group_uuid"));
        assert!(columns.iter().any(|column| column == "repeat_rule"));
        assert!(columns
            .iter()
            .any(|column| column == "repeat_next_due_date"));
        assert!(columns.iter().any(|column| column == "repeat_series_uuid"));
        let group_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'groups'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(group_tables, 1);
    }

    #[test]
    fn upgrades_v8_database_with_repeat_support() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();
        apply_migration(&mut connection, 7, add_reminder_deliveries).unwrap();
        apply_migration(&mut connection, 8, add_todo_groups).unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(todos)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "repeat_rule"));
        assert!(columns
            .iter()
            .any(|column| column == "repeat_next_due_date"));
        assert!(columns.iter().any(|column| column == "repeat_series_uuid"));
    }

    #[test]
    fn upgrades_v9_database_with_repeat_series_uuid() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();
        apply_migration(&mut connection, 7, add_reminder_deliveries).unwrap();
        apply_migration(&mut connection, 8, add_todo_groups).unwrap();
        apply_migration(&mut connection, 9, add_todo_repeats).unwrap();
        let uuid = "00000000-0000-4000-8000-000000000001";
        connection
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, pinned, sort_order, created_at, updated_at,
                    completed_at, deleted_at, due_date, due_at, reminder_at,
                    repeat_rule, repeat_next_due_date, updated_by
                )
                VALUES (?1, 'repeat', 0, 0, 0, 1, 2, NULL, NULL, '2026-06-10',
                    NULL, NULL, 'daily', '2026-06-11', ?2)
                ",
                params![uuid, device_id(&connection).unwrap()],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let series_uuid: String = connection
            .query_row("SELECT repeat_series_uuid FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(series_uuid, uuid);
    }

    #[test]
    fn upgrades_v10_database_with_note_field() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();
        apply_migration(&mut connection, 7, add_reminder_deliveries).unwrap();
        apply_migration(&mut connection, 8, add_todo_groups).unwrap();
        apply_migration(&mut connection, 9, add_todo_repeats).unwrap();
        apply_migration(&mut connection, 10, add_repeat_series_uuid).unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(todos)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "note"));
        assert!(columns.iter().any(|column| column == "archived_at"));
    }

    #[test]
    fn upgrades_v12_database_with_default_priority() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );
                ",
            )
            .unwrap();
        apply_migration(&mut connection, 1, create_legacy_schema).unwrap();
        apply_migration(&mut connection, 2, migrate_todos_for_sync).unwrap();
        apply_migration(&mut connection, 3, add_sync_identity).unwrap();
        apply_migration(&mut connection, 4, add_sync_settings).unwrap();
        apply_migration(&mut connection, 5, add_todo_pinning).unwrap();
        apply_migration(&mut connection, 6, add_todo_due_fields).unwrap();
        apply_migration(&mut connection, 7, add_reminder_deliveries).unwrap();
        apply_migration(&mut connection, 8, add_todo_groups).unwrap();
        apply_migration(&mut connection, 9, add_todo_repeats).unwrap();
        apply_migration(&mut connection, 10, add_repeat_series_uuid).unwrap();
        apply_migration(&mut connection, 11, add_todo_note).unwrap();
        apply_migration(&mut connection, 12, add_todo_archive).unwrap();
        connection
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, sort_order, created_at, updated_at, updated_by
                )
                VALUES (?1, 'priority default', 0, 0, 1, 2, ?2)
                ",
                params![
                    "00000000-0000-4000-8000-000000000001",
                    device_id(&connection).unwrap()
                ],
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(schema_version(&connection).unwrap(), CURRENT_SCHEMA_VERSION);
        let priority: i64 = connection
            .query_row("SELECT priority FROM todos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(priority, 0);
    }
}
