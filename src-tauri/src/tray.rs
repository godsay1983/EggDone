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

const RECENT_BLUR_DURATION: Duration = Duration::from_millis(350);
const DIALOG_CLOSE_GRACE: Duration = Duration::from_millis(500);
const INTERNAL_INTERACTION_GRACE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct PanelStateInner {
    last_blur_hide: Option<Instant>,
    last_tray_press: Option<Instant>,
    dialog_closed_at: Option<Instant>,
    dialog_active: bool,
    last_internal_interaction: Option<Instant>,
}

#[derive(Default)]
pub struct PanelState {
    inner: Mutex<PanelStateInner>,
}

impl PanelState {
    pub fn handle_blur(&self) -> bool {
        self.handle_blur_at(Instant::now())
    }

    pub fn begin_dialog(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.dialog_active = true;
        }
    }

    pub fn end_dialog(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.dialog_active = false;
            inner.dialog_closed_at = Some(Instant::now());
        }
    }

    pub fn mark_internal_interaction(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_internal_interaction = Some(Instant::now());
        }
    }

    fn mark_tray_press(&self) {
        self.mark_tray_press_at(Instant::now());
    }

    fn consume_tray_blur(&self) -> bool {
        self.consume_tray_blur_at(Instant::now())
    }

    fn clear_toggle_history(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_blur_hide = None;
            inner.last_tray_press = None;
        }
    }

    fn handle_blur_at(&self, now: Instant) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return true;
        };
        let dialog_grace = inner
            .dialog_closed_at
            .is_some_and(|closed_at| duration_since(now, closed_at) < DIALOG_CLOSE_GRACE);
        let interaction_grace = inner.last_internal_interaction.is_some_and(|interaction| {
            duration_since(now, interaction) < INTERNAL_INTERACTION_GRACE
        });

        if inner.dialog_active || dialog_grace || interaction_grace {
            return false;
        }

        inner.last_blur_hide = Some(now);
        true
    }

    fn mark_tray_press_at(&self, now: Instant) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_tray_press = Some(now);
        }
    }

    fn consume_tray_blur_at(&self, now: Instant) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let should_suppress = match (inner.last_blur_hide, inner.last_tray_press) {
            (Some(blur), Some(press)) => {
                duration_since(now, blur) < RECENT_BLUR_DURATION
                    && press <= blur
                    && duration_since(blur, press) < RECENT_BLUR_DURATION
            }
            _ => false,
        };
        inner.last_blur_hide = None;
        inner.last_tray_press = None;
        should_suppress
    }

    #[cfg(test)]
    fn begin_dialog_at(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.dialog_active = true;
        }
    }

    #[cfg(test)]
    fn end_dialog_at(&self, now: Instant) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.dialog_active = false;
            inner.dialog_closed_at = Some(now);
        }
    }

    #[cfg(test)]
    fn mark_internal_interaction_at(&self, now: Instant) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_internal_interaction = Some(now);
        }
    }
}

fn duration_since(later: Instant, earlier: Instant) -> Duration {
    later.checked_duration_since(earlier).unwrap_or_default()
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
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } => {
                tray.app_handle().state::<PanelState>().mark_tray_press();
            }
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let app = tray.app_handle();
                // Tauri exposes tray rectangles in physical pixels. The scale
                // argument is ignored for physical Position/Size variants.
                let position = rect.position.to_physical::<f64>(1.0);
                let size = rect.size.to_physical::<f64>(1.0);
                toggle_panel(
                    app,
                    Some(Rect {
                        x: position.x,
                        y: position.y,
                        width: size.width,
                        height: size.height,
                    }),
                );
            }
            _ => {}
        })
        .build(app)
}

pub(crate) fn toggle_panel(app: &AppHandle, anchor: Option<Rect>) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    let visible = window.is_visible().unwrap_or(false);

    if visible {
        app.state::<PanelState>().clear_toggle_history();
        let _ = window.hide();
        return false;
    }

    // If this tray press caused the panel's blur, keep the interaction as a
    // hide action. A later tray press after an unrelated blur should reopen it.
    if app.state::<PanelState>().consume_tray_blur() {
        return false;
    }

    show_panel(app, anchor);
    true
}

pub(crate) fn show_panel(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    app.state::<PanelState>().clear_toggle_history();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_blur_hides_without_suppressing_a_later_tray_press() {
        let state = PanelState::default();
        let start = Instant::now();

        assert!(state.handle_blur_at(start));
        state.mark_tray_press_at(start + Duration::from_millis(100));

        assert!(!state.consume_tray_blur_at(start + Duration::from_millis(120)));
    }

    #[test]
    fn tray_press_followed_by_blur_suppresses_the_matching_toggle() {
        let state = PanelState::default();
        let start = Instant::now();

        state.mark_tray_press_at(start);
        assert!(state.handle_blur_at(start + Duration::from_millis(20)));

        assert!(state.consume_tray_blur_at(start + Duration::from_millis(40)));
        assert!(!state.consume_tray_blur_at(start + Duration::from_millis(50)));
    }

    #[test]
    fn internal_pointer_interaction_temporarily_ignores_blur() {
        let state = PanelState::default();
        let start = Instant::now();

        state.mark_internal_interaction_at(start);

        assert!(!state.handle_blur_at(start + Duration::from_millis(100)));
        assert!(state.handle_blur_at(start + INTERNAL_INTERACTION_GRACE));
    }

    #[test]
    fn native_dialog_and_close_grace_ignore_blur() {
        let state = PanelState::default();
        let start = Instant::now();

        state.begin_dialog_at();
        assert!(!state.handle_blur_at(start));

        state.end_dialog_at(start + Duration::from_millis(20));
        assert!(!state.handle_blur_at(start + Duration::from_millis(200)));
        assert!(state.handle_blur_at(start + Duration::from_millis(520)));
    }
}
