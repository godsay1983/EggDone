use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition,
};

use crate::panel_position::{self, Rect, Size};

#[derive(Default)]
pub struct PanelState {
    last_blur_hide: Mutex<Option<Instant>>,
    dialog_closed_at: Mutex<Option<Instant>>,
    dialog_active: Mutex<bool>,
}

impl PanelState {
    pub fn mark_blur_hide(&self) {
        if let Ok(mut last_blur_hide) = self.last_blur_hide.lock() {
            *last_blur_hide = Some(Instant::now());
        }
    }

    pub fn begin_dialog(&self) {
        if let Ok(mut dialog_active) = self.dialog_active.lock() {
            *dialog_active = true;
        }
    }

    pub fn end_dialog(&self) {
        if let Ok(mut dialog_active) = self.dialog_active.lock() {
            *dialog_active = false;
        }
        if let Ok(mut dialog_closed_at) = self.dialog_closed_at.lock() {
            *dialog_closed_at = Some(Instant::now());
        }
    }

    pub fn should_keep_visible_on_blur(&self) -> bool {
        if self
            .dialog_active
            .lock()
            .map(|active| *active)
            .unwrap_or(false)
        {
            return true;
        }

        self.dialog_closed_at
            .lock()
            .ok()
            .and_then(|closed_at| *closed_at)
            .map(|instant| instant.elapsed() < Duration::from_millis(500))
            .unwrap_or(false)
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

pub fn create_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
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
            "toggle" => {
                toggle_panel(app, None);
            }
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
                // Tauri exposes tray rectangles in physical pixels. The scale
                // argument is ignored for physical Position/Size variants.
                let position = rect.position.to_physical::<f64>(1.0);
                let size = rect.size.to_physical::<f64>(1.0);
                let anchor = Rect {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                };
                toggle_panel(app, Some(anchor));
            }
        })
        .build(app)
}

pub(crate) fn toggle_panel(app: &AppHandle, anchor: Option<Rect>) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    let visible = window.is_visible().unwrap_or(false);

    if visible {
        let _ = window.hide();
        return false;
    }

    // On Windows, clicking the tray first blurs the panel. Keep that click as a
    // hide action instead of immediately reopening the panel.
    if app.state::<PanelState>().take_recent_blur_hide() {
        return false;
    }

    show_panel(app, anchor);
    true
}

pub(crate) fn show_panel(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    app.state::<PanelState>().clear_blur_hide();

    if let Some(anchor) = anchor {
        place_near_tray(&window, anchor);
    } else {
        place_at_screen_corner(&window);
    }

    let _ = window.show();
    let _ = window.set_focus();
}

fn place_near_tray(window: &tauri::WebviewWindow, anchor: Rect) {
    let Ok(panel_size) = window.outer_size() else {
        return;
    };
    let panel = Size {
        width: f64::from(panel_size.width),
        height: f64::from(panel_size.height),
    };
    let anchor_center = anchor.center();
    let monitor = window
        .monitor_from_point(anchor_center.x, anchor_center.y)
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        set_panel_position(
            window,
            panel_position::near_tray(
                anchor,
                monitor_work_area(&monitor),
                panel,
                monitor.scale_factor(),
            ),
        );
    }
}

fn place_at_screen_corner(window: &tauri::WebviewWindow) {
    let (Ok(panel_size), Ok(Some(monitor))) = (window.outer_size(), window.primary_monitor())
    else {
        return;
    };
    let panel = Size {
        width: f64::from(panel_size.width),
        height: f64::from(panel_size.height),
    };
    set_panel_position(
        window,
        panel_position::at_bottom_right(monitor_work_area(&monitor), panel, monitor.scale_factor()),
    );
}

fn monitor_work_area(monitor: &Monitor) -> Rect {
    let work_area = monitor.work_area();
    Rect {
        x: f64::from(work_area.position.x),
        y: f64::from(work_area.position.y),
        width: f64::from(work_area.size.width),
        height: f64::from(work_area.size.height),
    }
}

fn set_panel_position(window: &tauri::WebviewWindow, point: panel_position::Point) {
    let _ = window.set_position(PhysicalPosition::new(
        point.x.round() as i32,
        point.y.round() as i32,
    ));
}
