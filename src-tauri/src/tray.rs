use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition,
};

#[derive(Default)]
pub struct PanelState {
    last_blur_hide: Mutex<Option<Instant>>,
}

impl PanelState {
    pub fn mark_blur_hide(&self) {
        if let Ok(mut last_blur_hide) = self.last_blur_hide.lock() {
            *last_blur_hide = Some(Instant::now());
        }
    }

    fn take_recent_blur_hide(&self) -> bool {
        let Ok(mut last_blur_hide) = self.last_blur_hide.lock() else {
            return false;
        };
        let was_recent = last_blur_hide
            .map(|instant| instant.elapsed() < Duration::from_millis(350))
            .unwrap_or(false);
        *last_blur_hide = None;
        was_recent
    }

    fn clear_blur_hide(&self) {
        if let Ok(mut last_blur_hide) = self.last_blur_hide.lock() {
            *last_blur_hide = None;
        }
    }
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_item = MenuItem::with_id(app, "toggle", "打开 / 隐藏面板", true, None::<&str>)?;
    let new_item = MenuItem::with_id(app, "new", "新增任务", true, None::<&str>)?;
    let about_item = MenuItem::with_id(app, "about", "关于 EggDone", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&toggle_item, &new_item, &separator, &about_item, &quit_item],
    )?;

    let tray_icon = app
        .default_window_icon()
        .cloned()
        .expect("EggDone application icon is missing");

    TrayIconBuilder::with_id("eggdone-tray")
        .icon(tray_icon)
        .tooltip("蛋定 Todo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_panel(app, None),
            "new" => {
                show_panel(app, None);
                let _ = app.emit_to("main", "focus-new-todo", ());
            }
            "about" => {
                show_panel(app, None);
                let _ = app.emit_to("main", "show-about", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                let scale_factor = app
                    .get_webview_window("main")
                    .and_then(|window| window.scale_factor().ok())
                    .unwrap_or(1.0);
                let position = rect.position.to_physical::<f64>(scale_factor);
                let size = rect.size.to_physical::<f64>(scale_factor);
                let anchor = (position.x, position.y, size.width, size.height);
                toggle_panel(app, Some(anchor));
            }
        })
        .build(app)?;

    Ok(())
}

fn toggle_panel(app: &AppHandle, anchor: Option<(f64, f64, f64, f64)>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(false);

    if visible {
        let _ = window.hide();
        return;
    }

    // On Windows, clicking the tray first blurs the panel. Keep that click as a
    // hide action instead of immediately reopening the panel.
    if app.state::<PanelState>().take_recent_blur_hide() {
        return;
    }

    show_panel(app, anchor);
}

fn show_panel(app: &AppHandle, anchor: Option<(f64, f64, f64, f64)>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    app.state::<PanelState>().clear_blur_hide();

    if let Some((x, y, width, height)) = anchor {
        place_near_tray(&window, x, y, width, height);
    } else {
        place_at_screen_corner(&window);
    }

    let _ = window.show();
    let _ = window.set_focus();
}

fn place_near_tray(
    window: &tauri::WebviewWindow,
    tray_x: f64,
    tray_y: f64,
    tray_width: f64,
    tray_height: f64,
) {
    let Ok(panel_size) = window.outer_size() else {
        return;
    };
    let panel_width = f64::from(panel_size.width);
    let panel_height = f64::from(panel_size.height);

    let mut x = tray_x + tray_width - panel_width;
    let mut y = if tray_y >= panel_height {
        tray_y - panel_height - 8.0
    } else {
        tray_y + tray_height + 8.0
    };

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let position = monitor.position();
        let size = monitor.size();
        let min_x = f64::from(position.x) + 8.0;
        let min_y = f64::from(position.y) + 8.0;
        let max_x = f64::from(position.x) + f64::from(size.width) - panel_width - 8.0;
        let max_y = f64::from(position.y) + f64::from(size.height) - panel_height - 8.0;
        x = x.clamp(min_x, max_x.max(min_x));
        y = y.clamp(min_y, max_y.max(min_y));
    }

    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn place_at_screen_corner(window: &tauri::WebviewWindow) {
    let (Ok(panel_size), Ok(Some(monitor))) = (window.outer_size(), window.primary_monitor())
    else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x = monitor_position.x + monitor_size.width.saturating_sub(panel_size.width) as i32 - 16;
    let y = monitor_position.y + monitor_size.height.saturating_sub(panel_size.height) as i32 - 56;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
