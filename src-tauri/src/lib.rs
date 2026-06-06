mod commands;
mod db;
mod tray;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(tray::PanelState::default())
        .setup(|app| {
            let database = db::Database::open(app.handle())?;
            app.manage(database);
            tray::create_tray(app.handle())?;
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
                    // Keep the process alive and treat the panel like a native tray popover.
                    window
                        .app_handle()
                        .state::<tray::PanelState>()
                        .mark_blur_hide();
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_todos,
            commands::create_todo,
            commands::set_todo_completed,
            commands::delete_todo,
            commands::hide_panel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
