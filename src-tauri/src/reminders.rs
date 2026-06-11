use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::db::{device_id, now_millis, Database};

const REMINDER_CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
struct DueReminder {
    uuid: String,
    title: String,
    reminder_at: i64,
}

pub(crate) fn start_reminder_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        // Run once on startup so reminders missed while EggDone was closed are
        // delivered promptly, then keep a lightweight minute-level poll.
        check_due_reminders(&app);
        loop {
            std::thread::sleep(REMINDER_CHECK_INTERVAL);
            check_due_reminders(&app);
        }
    });
}

pub(crate) fn check_due_reminders(app: &AppHandle) {
    let database = app.state::<Database>();
    let Ok(connection) = database.connection.lock() else {
        return;
    };
    if let Err(error) = deliver_due_reminders(app, &connection, now_millis()) {
        eprintln!("reminder check failed: {error}");
    }
}

fn deliver_due_reminders(
    app: &AppHandle,
    connection: &Connection,
    now: i64,
) -> Result<usize, String> {
    let reminders = due_reminders(connection, now)?;
    let local_device_id = device_id(connection).map_err(database_error)?;
    let mut delivered = 0;

    for reminder in reminders {
        app.notification()
            .builder()
            .title("蛋定 Todo")
            .body(format!("该处理了：{}", reminder.title))
            .auto_cancel()
            .show()
            .map_err(|error| format!("发送系统提醒失败：{error}"))?;

        connection
            .execute(
                "
                INSERT OR IGNORE INTO reminder_deliveries (
                    todo_uuid, device_id, reminder_at, fired_at
                )
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![reminder.uuid, local_device_id, reminder.reminder_at, now],
            )
            .map_err(database_error)?;
        delivered += 1;
    }

    Ok(delivered)
}

fn due_reminders(connection: &Connection, now: i64) -> Result<Vec<DueReminder>, String> {
    let local_device_id = device_id(connection).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "
            SELECT uuid, title, reminder_at
            FROM todos
            WHERE completed = 0
              AND deleted_at IS NULL
              AND archived_at IS NULL
              AND reminder_at IS NOT NULL
              AND reminder_at <= ?1
              AND NOT EXISTS (
                  SELECT 1
                  FROM reminder_deliveries
                  WHERE reminder_deliveries.todo_uuid = todos.uuid
                    AND reminder_deliveries.device_id = ?2
                    AND reminder_deliveries.reminder_at = todos.reminder_at
              )
            ORDER BY reminder_at ASC, sort_order ASC, created_at ASC
            LIMIT 5
            ",
        )
        .map_err(database_error)?;

    let rows = statement
        .query_map(params![now, local_device_id], |row| {
            Ok(DueReminder {
                uuid: row.get(0)?,
                title: row.get(1)?,
                reminder_at: row.get(2)?,
            })
        })
        .map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};
    use uuid::Uuid;

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    fn insert_todo(
        connection: &Connection,
        title: &str,
        completed: bool,
        reminder_at: Option<i64>,
    ) -> String {
        let uuid = Uuid::new_v4().to_string();
        let device = device_id(connection).unwrap();
        connection
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, pinned, sort_order, created_at, updated_at,
                    completed_at, deleted_at, due_date, due_at, reminder_at, updated_by
                )
                VALUES (?1, ?2, ?3, 0, 0, 1, 1, NULL, NULL, NULL, NULL, ?4, ?5)
                ",
                params![uuid, title, completed, reminder_at, device],
            )
            .unwrap();
        uuid
    }

    #[test]
    fn finds_only_due_active_unfired_reminders() {
        let connection = connection();
        let due_uuid = insert_todo(&connection, "due", false, Some(10));
        insert_todo(&connection, "future", false, Some(30));
        insert_todo(&connection, "done", true, Some(10));
        insert_todo(&connection, "none", false, None);

        let reminders = due_reminders(&connection, 20).unwrap();

        assert_eq!(
            reminders,
            vec![DueReminder {
                uuid: due_uuid,
                title: "due".to_string(),
                reminder_at: 10,
            }]
        );
    }

    #[test]
    fn ignores_reminders_already_fired_on_this_device() {
        let connection = connection();
        let uuid = insert_todo(&connection, "due", false, Some(10));
        let device = device_id(&connection).unwrap();
        connection
            .execute(
                "
                INSERT INTO reminder_deliveries (
                    todo_uuid, device_id, reminder_at, fired_at
                )
                VALUES (?1, ?2, 10, 20)
                ",
                params![uuid, device],
            )
            .unwrap();

        assert!(due_reminders(&connection, 30).unwrap().is_empty());
    }
}
