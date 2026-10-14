//! Toolbar icons drawn as vector strokes, so they stay crisp at any display scaling.

use std::sync::OnceLock;

use eframe::egui::{Color32, CornerRadius, Painter, Rect, Sense, Shape, Stroke, Ui, vec2};

use crate::svg::{self, Part};
use crate::theme;

/// Lucide icons (lucide.dev), drawn from their SVG sources.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Prev,
    Next,
    ZoomIn,
    ZoomOut,
    Fit,
    RotateLeft,
    RotateRight,
    Play,
    Pause,
    Loop,
    Copy,
    Folder,
    OpenWith,
    Delete,
    Edit,
    Info,
    Settings,
    Gallery,
}

/// In `Icon` order.
pub const SOURCES: &[(&str, &str)] = &[
    ("chevron-left", include_str!("../assets/icons/chevron-left.svg")),
    ("chevron-right", include_str!("../assets/icons/chevron-right.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    ("minus", include_str!("../assets/icons/minus.svg")),
    ("scan", include_str!("../assets/icons/scan.svg")),
    ("rotate-ccw", include_str!("../assets/icons/rotate-ccw.svg")),
    ("rotate-cw", include_str!("../assets/icons/rotate-cw.svg")),
    ("play", include_str!("../assets/icons/play.svg")),
    ("pause", include_str!("../assets/icons/pause.svg")),
    ("repeat", include_str!("../assets/icons/repeat.svg")),
    ("copy", include_str!("../assets/icons/copy.svg")),
    ("folder-open", include_str!("../assets/icons/folder-open.svg")),
    ("app-window", include_str!("../assets/icons/app-window.svg")),
    ("trash", include_str!("../assets/icons/trash.svg")),
    ("square-pen", include_str!("../assets/icons/square-pen.svg")),
    ("info", include_str!("../assets/icons/info.svg")),
    ("settings", include_str!("../assets/icons/settings.svg")),
    ("layout-grid", include_str!("../assets/icons/layout-grid.svg")),
];

fn shapes(icon: Icon) -> &'static [Part] {
    static PARSED: OnceLock<Vec<Vec<Part>>> = OnceLock::new();
    &PARSED.get_or_init(|| SOURCES.iter().map(|(_, src)| svg::parse(src)).collect())[icon as usize]
}

/// A square icon button; true when clicked. `active` highlights it (for toggles).
pub fn button(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let fill = if response.is_pointer_button_down_on() && enabled {
            theme::pal().button_down
        } else if hovered || active {
            theme::pal().button_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
        let color = if !enabled {
            theme::pal().text_disabled
        } else if active {
            theme::pal().accent
        } else if hovered {
            theme::pal().text
        } else {
            theme::pal().text_dim
        };
        draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), color);
    }
    enabled && response.on_hover_text(tip).clicked()
}

/// Draw `icon` (a 24-unit design) to fill `r`, with Lucide's 2-unit stroke.
pub fn draw(p: &Painter, icon: Icon, r: Rect, color: Color32) {
    let scale = r.width().min(r.height()) / 24.0;
    let origin = r.center() - vec2(12.0, 12.0) * scale;
    let at = |q: eframe::egui::Pos2| origin + q.to_vec2() * scale;
    let width = 2.0 * scale;
    for part in shapes(icon) {
        match part {
            Part::Line { points, closed } => {
                let pts: Vec<_> = points.iter().map(|&q| at(q)).collect();
                if !*closed {
                    // Round caps, as the icons are designed.
                    for end in [pts[0], pts[pts.len() - 1]] {
                        p.circle_filled(end, width / 2.0, color);
                    }
                }
                let stroke = Stroke::new(width, color);
                p.add(if *closed { Shape::closed_line(pts, stroke) } else { Shape::line(pts, stroke) });
            }
            Part::Dot { centre, radius } => {
                p.circle_filled(at(*centre), (radius * scale).max(width / 2.0), color);
            }
        }
    }
}

/// An icon followed by a label, for the one prominent action.
pub fn text_button(ui: &mut Ui, icon: Icon, text: &str, tip: &str, enabled: bool) -> bool {
    let font = eframe::egui::FontId::proportional(14.0);
    let color = if enabled { theme::pal().text } else { theme::pal().text_disabled };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = vec2(11.0 + 16.0 + 8.0 + galley.size().x + 12.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.is_pointer_button_down_on() {
            theme::pal().button_down
        } else if enabled && response.hovered() {
            theme::pal().button_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
        let icon_rect = Rect::from_min_size(rect.left_center() + vec2(11.0, -8.0), vec2(16.0, 16.0));
        draw(ui.painter(), icon, icon_rect, color);
        let text_pos = rect.left_center() + vec2(11.0 + 16.0 + 8.0, -galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, color);
    }
    enabled && response.on_hover_text(tip).clicked()
}
