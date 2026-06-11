use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tauri::{
    image::Image,
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, Wry,
};

use crate::{
    db::Database,
    panel_position::{self, Rect, Size},
};

const RECENT_BLUR_DURATION: Duration = Duration::from_millis(350);
const DIALOG_CLOSE_GRACE: Duration = Duration::from_millis(500);
const INTERNAL_INTERACTION_GRACE: Duration = Duration::from_millis(300);
const TRAY_ID: &str = "eggdone-tray";
const TODAY_TASK_MENU_LIMIT: usize = 3;
const TODAY_TASK_MENU_TITLE_MAX_CHARS: usize = 18;

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
    let menu = build_tray_menu(app, &[])?;

    let tray_icon = app
        .default_window_icon()
        .cloned()
        .expect("EggDone application icon is missing");

    let tray = TrayIconBuilder::with_id(TRAY_ID)
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
            "today" => {
                show_panel(app, None);
                let _ = app.emit_to("main", "show-today", ());
            }
            id if id.starts_with("today-task-") => {
                show_panel(app, None);
                let _ = app.emit_to("main", "show-today", ());
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
        .build(app)?;
    update_task_badge(app);
    Ok(tray)
}

fn build_tray_menu(app: &AppHandle, today_task_titles: &[String]) -> tauri::Result<Menu<Wry>> {
    let toggle_item = MenuItem::with_id(app, "toggle", "打开 / 隐藏面板", true, None::<&str>)?;
    let new_item = MenuItem::with_id(app, "new", "新增任务", true, None::<&str>)?;
    let today_item = MenuItem::with_id(app, "today", "今天任务", true, None::<&str>)?;
    let preview_separator = PredefinedMenuItem::separator(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let about_item = MenuItem::with_id(app, "about", "关于 EggDone", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let preview_items = today_task_titles
        .iter()
        .take(TODAY_TASK_MENU_LIMIT)
        .enumerate()
        .map(|(index, title)| {
            MenuItem::with_id(
                app,
                format!("today-task-{index}"),
                today_task_menu_label(index, title),
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;

    let mut items: Vec<&dyn IsMenuItem<_>> = vec![&toggle_item, &new_item, &today_item];
    if !preview_items.is_empty() {
        items.push(&preview_separator);
        for item in &preview_items {
            items.push(item);
        }
    }
    items.push(&separator);
    items.push(&about_item);
    items.push(&quit_item);

    Menu::with_items(app, &items)
}

pub(crate) fn update_task_badge(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let database = app.state::<Database>();
    let Ok(connection) = database.connection.lock() else {
        return;
    };
    let counts = connection.query_row(
        "
        SELECT
            SUM(CASE WHEN completed = 0 THEN 1 ELSE 0 END),
            COUNT(*),
            SUM(
                CASE
                    WHEN completed = 0
                        AND (
                            due_date <= date('now', 'localtime')
                            OR date(due_at / 1000, 'unixepoch', 'localtime') <= date('now', 'localtime')
                        )
                    THEN 1 ELSE 0
                END
            )
        FROM todos
        WHERE deleted_at IS NULL
        ",
        [],
        |row| {
            Ok((
                row.get::<_, Option<u32>>(0)?.unwrap_or(0),
                row.get(1)?,
                row.get::<_, Option<u32>>(2)?.unwrap_or(0),
            ))
        },
    );
    let Ok((remaining, total, today_due)) = counts else {
        return;
    };
    let today_task_titles =
        today_task_titles(&connection, TODAY_TASK_MENU_LIMIT).unwrap_or_default();
    drop(connection);

    let Some(base) = app.default_window_icon() else {
        return;
    };
    let badge = draw_task_badge(base, remaining, total);
    let _ = tray.set_icon(Some(badge));
    let _ = tray.set_tooltip(Some(task_tooltip(remaining, total, today_due)));
    if let Ok(menu) = build_tray_menu(app, &today_task_titles) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn task_tooltip(remaining: u32, total: u32, today_due: u32) -> String {
    format!("蛋定 Todo · {remaining}/{total} 项未完成 · 今天 {today_due} 项")
}

fn today_task_titles(
    connection: &rusqlite::Connection,
    limit: usize,
) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "
        SELECT title
        FROM todos
        WHERE deleted_at IS NULL
            AND completed = 0
            AND (
                due_date <= date('now', 'localtime')
                OR date(due_at / 1000, 'unixepoch', 'localtime') <= date('now', 'localtime')
            )
        ORDER BY
            pinned DESC,
            COALESCE(due_date, date(due_at / 1000, 'unixepoch', 'localtime')) ASC,
            sort_order ASC,
            created_at ASC,
            id ASC
        LIMIT ?1
        ",
    )?;
    let rows = statement.query_map(rusqlite::params![limit as i64], |row| row.get(0))?;
    rows.collect()
}

fn today_task_menu_label(index: usize, title: &str) -> String {
    format!(
        "{}. {}",
        index + 1,
        truncate_menu_title(title, TODAY_TASK_MENU_TITLE_MAX_CHARS)
    )
}

fn truncate_menu_title(title: &str, max_chars: usize) -> String {
    let title = title.trim();
    if title.chars().count() <= max_chars {
        return title.to_string();
    }

    let keep = max_chars.saturating_sub(3);
    format!("{}...", title.chars().take(keep).collect::<String>())
}

fn draw_task_badge(base: &Image<'_>, remaining: u32, total: u32) -> Image<'static> {
    let width = base.width();
    let height = base.height();
    let mut rgba = base.rgba().to_vec();
    let scale = (height / 16).max(1);
    let text = compact_badge_text(remaining, total);
    let text_width = text_width(&text, scale);
    let badge_height = (7 * scale).min(height);
    let badge_width = (text_width + 4 * scale).min(width);
    let left = width.saturating_sub(badge_width);
    let top = height.saturating_sub(badge_height);

    fill_rect(
        &mut rgba,
        width,
        height,
        left,
        top,
        badge_width,
        badge_height,
        [255, 249, 229, 255],
    );
    fill_rect(
        &mut rgba,
        width,
        height,
        left + scale.min(badge_width),
        top + scale.min(badge_height),
        badge_width.saturating_sub(scale * 2),
        badge_height.saturating_sub(scale * 2),
        [246, 201, 76, 255],
    );
    let text_left = left + badge_width.saturating_sub(text_width) / 2;
    let text_top = top + scale;
    draw_text(
        &mut rgba,
        width,
        height,
        text_left,
        text_top,
        scale,
        &text,
        [82, 61, 25, 255],
    );

    Image::new_owned(rgba, width, height)
}

fn compact_badge_text(remaining: u32, total: u32) -> String {
    if remaining <= 9 && total <= 9 {
        format!("{remaining}/{total}")
    } else if remaining == 0 {
        "0".to_string()
    } else {
        "9+".to_string()
    }
}

fn text_width(text: &str, scale: u32) -> u32 {
    let count = text.chars().count() as u32;
    if count == 0 {
        0
    } else {
        (count * 3 + count - 1) * scale
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    rgba: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    color: [u8; 4],
) {
    for y in top..top.saturating_add(height).min(canvas_height) {
        for x in left..left.saturating_add(width).min(canvas_width) {
            set_pixel(rgba, canvas_width, x, y, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    rgba: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    left: u32,
    top: u32,
    scale: u32,
    text: &str,
    color: [u8; 4],
) {
    let mut cursor = left;
    for character in text.chars() {
        let glyph = glyph(character);
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) == 0 {
                    continue;
                }
                fill_rect(
                    rgba,
                    canvas_width,
                    canvas_height,
                    cursor + column * scale,
                    top + row as u32 * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
        cursor += 4 * scale;
    }
}

fn glyph(character: char) -> [u8; 5] {
    match character {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => [0; 5],
    }
}

fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 4]) {
    let index = ((y * width + x) * 4) as usize;
    if let Some(pixel) = rgba.get_mut(index..index + 4) {
        pixel.copy_from_slice(&color);
    }
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

    #[test]
    fn formats_single_digit_ratios_and_compacts_larger_counts() {
        assert_eq!(compact_badge_text(3, 4), "3/4");
        assert_eq!(compact_badge_text(0, 12), "0");
        assert_eq!(compact_badge_text(12, 15), "9+");
    }

    #[test]
    fn draws_badge_without_changing_image_dimensions() {
        let base = Image::new_owned(vec![0; 32 * 32 * 4], 32, 32);
        let badge = draw_task_badge(&base, 3, 4);

        assert_eq!(badge.width(), 32);
        assert_eq!(badge.height(), 32);
        assert_ne!(badge.rgba(), base.rgba());
    }

    #[test]
    fn tooltip_mentions_today_due_count() {
        assert_eq!(
            task_tooltip(3, 4, 2),
            "蛋定 Todo · 3/4 项未完成 · 今天 2 项"
        );
    }

    #[test]
    fn formats_today_task_menu_labels_compactly() {
        assert_eq!(today_task_menu_label(0, "写周报"), "1. 写周报");
        assert_eq!(
            today_task_menu_label(1, "这是一个非常非常非常长的任务标题需要截断"),
            "2. 这是一个非常非常非常长的任务标..."
        );
    }
}
