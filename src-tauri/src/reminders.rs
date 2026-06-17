use std::time::Duration;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

use crate::{
    db::{device_id, now_millis, Database},
    tray,
};

const REMINDER_CHECK_INTERVAL: Duration = Duration::from_secs(60);
const TEN_MINUTES_MILLIS: i64 = 10 * 60 * 1000;
const TWO_HOURS_MILLIS: i64 = 2 * 60 * 60 * 1000;
const LATER_TODAY_HOUR: u8 = 18;

#[derive(Debug, PartialEq, Eq)]
struct DueReminder {
    id: i64,
    uuid: String,
    title: String,
    reminder_at: i64,
}

#[derive(Clone, Serialize)]
struct FocusTodoPayload {
    uuid: String,
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
        deliver_system_notification(app, &reminder)?;

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
            SELECT id, uuid, title, reminder_at
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
                id: row.get(0)?,
                uuid: row.get(1)?,
                title: row.get(2)?,
                reminder_at: row.get(3)?,
            })
        })
        .map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

#[cfg(target_os = "windows")]
fn deliver_system_notification(app: &AppHandle, reminder: &DueReminder) -> Result<(), String> {
    use tauri_winrt_notification::{Duration as ToastDuration, Toast};

    let app_id = if tauri::is_dev() {
        Toast::POWERSHELL_APP_ID
    } else {
        &app.config().identifier
    };
    let click_uuid = reminder.uuid.clone();
    let snooze_uuid = reminder.uuid.clone();
    let later_uuid = reminder.uuid.clone();
    let app_for_click = app.clone();
    let app_for_snooze = app.clone();
    let app_for_later = app.clone();

    Toast::new(app_id)
        .title("蛋定 Todo")
        .text1(&format!("该处理了：{}", reminder.title))
        .duration(ToastDuration::Short)
        .add_button("稍后 10 分钟", "snooze-10")
        .add_button("今天晚些时候", "later-today")
        .on_activated(move |action| {
            match action.as_deref() {
                Some("snooze-10") => {
                    snooze_reminder(&app_for_snooze, &snooze_uuid, snooze_reminder_at());
                }
                Some("later-today") => {
                    snooze_reminder(&app_for_later, &later_uuid, later_today_reminder_at());
                }
                _ => focus_todo_from_notification(&app_for_click, &click_uuid),
            }
            Ok(())
        })
        .show()
        .map_err(|error| format!("发送系统提醒失败：{error}"))
}

#[cfg(not(target_os = "windows"))]
fn deliver_system_notification(app: &AppHandle, reminder: &DueReminder) -> Result<(), String> {
    app.notification()
        .builder()
        .title("蛋定 Todo")
        .body(format!("该处理了：{}", reminder.title))
        .auto_cancel()
        .show()
        .map_err(|error| format!("发送系统提醒失败：{error}"))
}

pub(crate) fn deliver_focus_notification(
    app: &AppHandle,
    completed_phase: &str,
) -> Result<(), String> {
    let body = if completed_phase == "break" {
        "休息结束，可以开始下一轮专注。"
    } else {
        "专注结束，先休息一下。"
    };
    deliver_focus_system_notification(app, body)
}

#[cfg(target_os = "windows")]
fn deliver_focus_system_notification(app: &AppHandle, body: &str) -> Result<(), String> {
    use tauri_winrt_notification::{Duration as ToastDuration, Toast};

    let app_id = if tauri::is_dev() {
        Toast::POWERSHELL_APP_ID
    } else {
        &app.config().identifier
    };

    Toast::new(app_id)
        .title("蛋定专注")
        .text1(body)
        .duration(ToastDuration::Short)
        .show()
        .map_err(|error| format!("发送专注提醒失败：{error}"))
}

#[cfg(not(target_os = "windows"))]
fn deliver_focus_system_notification(app: &AppHandle, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title("蛋定专注")
        .body(body)
        .auto_cancel()
        .show()
        .map_err(|error| format!("发送专注提醒失败：{error}"))
}

fn focus_todo_from_notification(app: &AppHandle, uuid: &str) {
    tray::show_panel(app, None);
    let _ = app.emit_to(
        "main",
        "focus-todo",
        FocusTodoPayload {
            uuid: uuid.to_string(),
        },
    );
}

fn snooze_reminder(app: &AppHandle, uuid: &str, reminder_at: i64) {
    let database = app.state::<Database>();
    let Ok(connection) = database.connection.lock() else {
        return;
    };
    let Ok(local_device_id) = device_id(&connection) else {
        return;
    };
    let now = now_millis();
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET reminder_at = ?1, updated_at = ?2, updated_by = ?3
            WHERE uuid = ?4
              AND completed = 0
              AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
            params![reminder_at, now, local_device_id, uuid],
        )
        .unwrap_or(0);

    if changed > 0 {
        drop(connection);
        tray::update_task_badge(app);
        let _ = app.emit_to("main", "todos-changed", ());
    }
}

fn snooze_reminder_at() -> i64 {
    now_millis() + TEN_MINUTES_MILLIS
}

fn later_today_reminder_at() -> i64 {
    let now = now_millis();
    let later_today = local_today_at_hour(now, LATER_TODAY_HOUR);
    if later_today > now {
        later_today
    } else {
        now + TWO_HOURS_MILLIS
    }
}

fn local_today_at_hour(timestamp: i64, hour: u8) -> i64 {
    let Ok(now_utc) = time::OffsetDateTime::from_unix_timestamp_nanos(
        i128::from(timestamp).saturating_mul(1_000_000),
    ) else {
        return timestamp;
    };
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let local = now_utc.to_offset(offset);
    let Ok(local_target) = local.date().with_hms(hour, 0, 0) else {
        return timestamp;
    };
    (local_target.assume_offset(offset).unix_timestamp_nanos() / 1_000_000) as i64
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
                id: 1,
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

    #[test]
    fn snooze_reminder_uses_ten_minutes_from_now() {
        let before = now_millis();
        let reminder_at = snooze_reminder_at();
        let after = now_millis();

        assert!(reminder_at >= before + TEN_MINUTES_MILLIS);
        assert!(reminder_at <= after + TEN_MINUTES_MILLIS);
    }

    #[test]
    fn later_today_reminder_is_in_the_future() {
        assert!(later_today_reminder_at() > now_millis());
    }
}
