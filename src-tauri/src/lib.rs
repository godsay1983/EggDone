mod commands;
mod data_exchange;
mod db;
mod panel_position;
mod reminders;
mod s3_sync;
mod sync;
mod tray;

use serde::Serialize;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

#[cfg(desktop)]
#[derive(Clone, Serialize)]
struct SingleInstancePayload {
    args: Vec<String>,
    cwd: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // The single-instance plugin must be registered before every other plugin.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
        tray::show_panel(app, None);
        let _ = app.emit_to(
            "main",
            "single-instance",
            SingleInstancePayload { args, cwd },
        );
        let _ = app.emit_to("main", "focus-new-todo", ());
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .manage(tray::PanelState::default())
        .manage(s3_sync::SyncRuntime::default())
        .setup(|app| {
            let database = db::Database::open(app.handle())?;
            app.manage(database);
            // Tauri removes a tray icon when its last handle is dropped.
            // Store the handle in application state for the whole process lifetime.
            let tray_icon = tray::create_tray(app.handle())?;
            app.manage(tray_icon);
            reminders::start_reminder_scheduler(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::Focused(false) => {
                    let panel_state = window.app_handle().state::<tray::PanelState>();
                    if !panel_state.handle_blur() {
                        return;
                    }
                    // Keep the process alive and treat the panel like a native tray popover.
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_todos,
            commands::list_groups,
            commands::create_todo,
            commands::create_group,
            commands::update_group_name,
            commands::update_group_color,
            commands::delete_group,
            commands::reorder_groups,
            commands::set_todo_completed,
            commands::update_todo_title,
            commands::set_todo_pinned,
            commands::set_todo_schedule,
            commands::set_todo_group,
            commands::reorder_todos,
            commands::delete_todo,
            commands::restore_todo,
            commands::clear_completed_todos,
            commands::hide_panel,
            commands::mark_panel_interaction,
            commands::toggle_panel_from_shortcut,
            commands::prepare_sync_document,
            commands::apply_remote_sync_document,
            commands::get_sync_settings,
            commands::save_sync_settings,
            commands::delete_sync_credentials,
            commands::test_sync_connection,
            commands::sync_now,
            data_exchange::export_todos,
            data_exchange::preview_todo_import,
            data_exchange::confirm_todo_import,
            data_exchange::backup_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
