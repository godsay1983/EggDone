const PANEL_GAP: f64 = 8.0;
const WORK_AREA_MARGIN: f64 = 8.0;
const CORNER_MARGIN: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

impl Rect {
    pub(crate) fn center(self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }

    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }
}

#[derive(Clone, Copy)]
enum ScreenEdge {
    Top,
    Right,
    Bottom,
    Left,
}

pub(crate) fn near_tray(anchor: Rect, work_area: Rect, panel: Size, scale_factor: f64) -> Point {
    let gap = PANEL_GAP * scale_factor;
    let margin = WORK_AREA_MARGIN * scale_factor;
    let edge = nearest_edge(anchor.center(), work_area);
    let candidate = match edge {
        ScreenEdge::Top => Point {
            x: anchor.x,
            y: anchor.bottom() + gap,
        },
        ScreenEdge::Right => Point {
            x: anchor.x - panel.width - gap,
            y: anchor.center().y - panel.height / 2.0,
        },
        ScreenEdge::Bottom => Point {
            x: anchor.right() - panel.width,
            y: anchor.y - panel.height - gap,
        },
        ScreenEdge::Left => Point {
            x: anchor.right() + gap,
            y: anchor.center().y - panel.height / 2.0,
        },
    };

    clamp_to_work_area(candidate, work_area, panel, margin)
}

pub(crate) fn at_bottom_right(work_area: Rect, panel: Size, scale_factor: f64) -> Point {
    let corner_margin = CORNER_MARGIN * scale_factor;
    clamp_to_work_area(
        Point {
            x: work_area.right() - panel.width - corner_margin,
            y: work_area.bottom() - panel.height - corner_margin,
        },
        work_area,
        panel,
        WORK_AREA_MARGIN * scale_factor,
    )
}

fn nearest_edge(point: Point, work_area: Rect) -> ScreenEdge {
    let distances = [
        ((point.y - work_area.y).abs(), ScreenEdge::Top),
        ((work_area.right() - point.x).abs(), ScreenEdge::Right),
        ((work_area.bottom() - point.y).abs(), ScreenEdge::Bottom),
        ((point.x - work_area.x).abs(), ScreenEdge::Left),
    ];

    distances
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, edge)| edge)
        .unwrap_or(ScreenEdge::Bottom)
}

fn clamp_to_work_area(point: Point, work_area: Rect, panel: Size, margin: f64) -> Point {
    let min_x = work_area.x + margin;
    let min_y = work_area.y + margin;
    let max_x = (work_area.right() - panel.width - margin).max(min_x);
    let max_y = (work_area.bottom() - panel.height - margin).max(min_y);

    Point {
        x: point.x.clamp(min_x, max_x),
        y: point.y.clamp(min_y, max_y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL: Size = Size {
        width: 360.0,
        height: 560.0,
    };

    #[test]
    fn opens_above_a_bottom_taskbar_tray() {
        let position = near_tray(
            Rect {
                x: 1880.0,
                y: 1040.0,
                width: 24.0,
                height: 24.0,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1040.0,
            },
            PANEL,
            1.0,
        );

        assert_eq!(
            position,
            Point {
                x: 1544.0,
                y: 472.0
            }
        );
    }

    #[test]
    fn opens_below_a_top_menu_bar_tray() {
        let position = near_tray(
            Rect {
                x: 1400.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
            },
            Rect {
                x: 0.0,
                y: 24.0,
                width: 1440.0,
                height: 876.0,
            },
            PANEL,
            1.0,
        );

        assert_eq!(position, Point { x: 1072.0, y: 32.0 });
    }

    #[test]
    fn opens_beside_a_left_taskbar_and_supports_negative_monitor_coordinates() {
        let position = near_tray(
            Rect {
                x: -1920.0,
                y: 900.0,
                width: 40.0,
                height: 24.0,
            },
            Rect {
                x: -1880.0,
                y: 0.0,
                width: 1880.0,
                height: 1080.0,
            },
            PANEL,
            1.0,
        );

        assert_eq!(
            position,
            Point {
                x: -1872.0,
                y: 512.0
            }
        );
    }

    #[test]
    fn opens_left_of_a_right_taskbar() {
        let position = near_tray(
            Rect {
                x: 1880.0,
                y: 500.0,
                width: 40.0,
                height: 24.0,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1880.0,
                height: 1080.0,
            },
            PANEL,
            1.0,
        );

        assert_eq!(
            position,
            Point {
                x: 1512.0,
                y: 232.0
            }
        );
    }

    #[test]
    fn keeps_an_oversized_panel_at_the_work_area_origin_margin() {
        let position = at_bottom_right(
            Rect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 400.0,
            },
            PANEL,
            1.0,
        );

        assert_eq!(position, Point { x: 108.0, y: 58.0 });
    }

    #[test]
    fn scales_visual_spacing_for_high_dpi_monitors() {
        let position = near_tray(
            Rect {
                x: 3760.0,
                y: 2080.0,
                width: 48.0,
                height: 48.0,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                width: 3840.0,
                height: 2080.0,
            },
            Size {
                width: 720.0,
                height: 1120.0,
            },
            2.0,
        );

        assert_eq!(
            position,
            Point {
                x: 3088.0,
                y: 944.0
            }
        );
    }
}
