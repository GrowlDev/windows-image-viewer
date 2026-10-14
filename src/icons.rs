//! Toolbar icons drawn as vector strokes, so they stay crisp at any display scaling.

use std::f32::consts::{PI, TAU};

use eframe::egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};

use crate::theme;

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
    External,
    Info,
}

/// A square icon button; true when clicked. `active` highlights it (for toggles).
pub fn button(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let fill = if response.is_pointer_button_down_on() && enabled {
            theme::BUTTON_DOWN
        } else if hovered || active {
            theme::BUTTON_HOVER
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
        let color = if !enabled {
            theme::TEXT_DISABLED
        } else if active {
            theme::ACCENT
        } else if hovered {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        draw(ui.painter(), icon, rect.shrink(8.0), color);
    }
    enabled && response.on_hover_text(tip).clicked()
}

pub fn draw(p: &Painter, icon: Icon, r: Rect, color: Color32) {
    let s = Stroke::new(1.5, color);
    let c = r.center();
    let (w, h) = (r.width(), r.height());
    let at = |x: f32, y: f32| pos2(r.left() + x * w, r.top() + y * h);
    let line = |pts: &[Pos2]| {
        p.add(Shape::line(pts.to_vec(), s));
    };
    match icon {
        Icon::Prev => line(&[at(0.65, 0.1), at(0.3, 0.5), at(0.65, 0.9)]),
        Icon::Next => line(&[at(0.35, 0.1), at(0.7, 0.5), at(0.35, 0.9)]),
        Icon::ZoomOut => line(&[at(0.1, 0.5), at(0.9, 0.5)]),
        Icon::ZoomIn => {
            line(&[at(0.1, 0.5), at(0.9, 0.5)]);
            line(&[at(0.5, 0.1), at(0.5, 0.9)]);
        }
        Icon::Fit => {
            for (x, y, dx, dy) in [(0.0, 0.0, 1.0, 1.0), (1.0, 0.0, -1.0, 1.0), (0.0, 1.0, 1.0, -1.0), (1.0, 1.0, -1.0, -1.0)] {
                line(&[at(x + dx * 0.35, y), at(x, y), at(x, y + dy * 0.35)]);
            }
        }
        Icon::RotateLeft | Icon::RotateRight => {
            // A three-quarter circle ending at the top, arrowhead showing the turn.
            let (from, to) = if icon == Icon::RotateRight { (0.0, 1.5 * PI) } else { (PI, -0.5 * PI) };
            let radius = w * 0.42;
            line(&arc(c, radius, from, to));
            let end = c + angle(to) * radius;
            let tangent = vec2(-to.sin(), to.cos()) * (to - from).signum();
            let radial = angle(to);
            let back = end - tangent * 3.5;
            line(&[back + radial * 3.0, end + tangent * 0.5, back - radial * 3.0]);
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(0.25, 0.1), at(0.85, 0.5), at(0.25, 0.9)], color, Stroke::NONE));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(0.2, 0.12), at(0.4, 0.88)), 1.0, color);
            p.rect_filled(Rect::from_min_max(at(0.6, 0.12), at(0.8, 0.88)), 1.0, color);
        }
        Icon::Loop => {
            let rr = Rect::from_min_max(at(0.05, 0.22), at(0.95, 0.78));
            p.rect_stroke(rr, CornerRadius::same(4), s, StrokeKind::Middle);
            p.rect_filled(Rect::from_center_size(pos2(c.x + 1.0, rr.top()), vec2(4.0, 3.0)), 0.0, theme::BAR);
            let t = pos2(c.x + 2.5, rr.top());
            line(&[t + vec2(-2.5, -2.5), t, t + vec2(-2.5, 2.5)]);
        }
        Icon::Copy => {
            p.rect_stroke(Rect::from_min_max(at(0.3, 0.3), at(0.95, 0.95)), CornerRadius::same(2), s, StrokeKind::Middle);
            line(&[at(0.7, 0.12), at(0.7, 0.05), at(0.05, 0.05), at(0.05, 0.7), at(0.12, 0.7)]);
        }
        Icon::Folder => {
            line(&[at(0.05, 0.85), at(0.05, 0.15), at(0.38, 0.15), at(0.5, 0.3), at(0.95, 0.3), at(0.95, 0.85), at(0.05, 0.85)]);
        }
        Icon::OpenWith => {
            for (x, y) in [(0.05, 0.05), (0.58, 0.05), (0.05, 0.58)] {
                p.rect_stroke(Rect::from_min_size(at(x, y), vec2(w * 0.37, h * 0.37)), CornerRadius::same(1), s, StrokeKind::Middle);
            }
            line(&[at(0.765, 0.58), at(0.765, 0.95)]);
            line(&[at(0.58, 0.765), at(0.95, 0.765)]);
        }
        Icon::Delete => {
            line(&[at(0.05, 0.2), at(0.95, 0.2)]);
            line(&[at(0.35, 0.2), at(0.38, 0.05), at(0.62, 0.05), at(0.65, 0.2)]);
            line(&[at(0.15, 0.2), at(0.22, 0.95), at(0.78, 0.95), at(0.85, 0.2)]);
            line(&[at(0.42, 0.38), at(0.42, 0.78)]);
            line(&[at(0.58, 0.38), at(0.58, 0.78)]);
        }
        Icon::External => {
            line(&[at(0.45, 0.1), at(0.1, 0.1), at(0.1, 0.9), at(0.9, 0.9), at(0.9, 0.55)]);
            line(&[at(0.45, 0.55), at(0.92, 0.08)]);
            line(&[at(0.6, 0.08), at(0.92, 0.08), at(0.92, 0.4)]);
        }
        Icon::Info => {
            p.circle_stroke(c, w * 0.45, s);
            p.circle_filled(at(0.5, 0.28), 1.2, color);
            line(&[at(0.5, 0.45), at(0.5, 0.75)]);
        }
    }
}

fn angle(a: f32) -> Vec2 {
    vec2(a.cos(), a.sin())
}

fn arc(c: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = ((to - from).abs() / TAU * 32.0).ceil().max(2.0) as usize;
    (0..=steps).map(|i| c + angle(from + (to - from) * i as f32 / steps as f32) * r).collect()
}

/// An icon followed by a label, for the one prominent action.
pub fn text_button(ui: &mut Ui, icon: Icon, text: &str, tip: &str, enabled: bool) -> bool {
    let font = eframe::egui::FontId::proportional(14.0);
    let color = if enabled { theme::TEXT } else { theme::TEXT_DISABLED };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = vec2(12.0 + 14.0 + 8.0 + galley.size().x + 12.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.is_pointer_button_down_on() {
            theme::BUTTON_DOWN
        } else if enabled && response.hovered() {
            theme::BUTTON_HOVER
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
        let icon_rect = Rect::from_min_size(rect.left_center() + vec2(12.0, -7.0), vec2(14.0, 14.0));
        draw(ui.painter(), icon, icon_rect, color);
        let text_pos = rect.left_center() + vec2(12.0 + 14.0 + 8.0, -galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, color);
    }
    enabled && response.on_hover_text(tip).clicked()
}
