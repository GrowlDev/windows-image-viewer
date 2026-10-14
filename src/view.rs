//! Zoom and pan, all in *physical* pixels so 100% means one image pixel per screen pixel
//! regardless of Windows display scaling.

use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};

pub const MIN_ZOOM: f32 = 1.0 / 32.0;
pub const MAX_ZOOM: f32 = 64.0;

/// Wheel/keyboard zoom stops: exact fractions below 100%, whole multiples above it.
const STEPS: &[f32] = &[
    1.0 / 32.0,
    1.0 / 24.0,
    1.0 / 16.0,
    1.0 / 12.0,
    1.0 / 8.0,
    1.0 / 6.0,
    1.0 / 4.0,
    1.0 / 3.0,
    1.0 / 2.0,
    2.0 / 3.0,
    1.0,
    2.0,
    3.0,
    4.0,
    5.0,
    6.0,
    7.0,
    8.0,
    10.0,
    12.0,
    14.0,
    16.0,
    20.0,
    24.0,
    28.0,
    32.0,
    40.0,
    48.0,
    56.0,
    64.0,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Follow the window: recompute the fit zoom whenever the canvas changes.
    pub fit: bool,
    /// Screen pixels per image pixel.
    pub zoom: f32,
    /// Offset of the image centre from the canvas centre, in screen pixels.
    pub pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self { fit: true, zoom: 1.0, pan: Vec2::ZERO }
    }
}

/// Shrink large images to fit; enlarge small ones by the largest whole multiple that fits,
/// so pixel art always lands on an exact grid.
pub fn fit_zoom(image: Vec2, avail: Vec2) -> f32 {
    if image.x <= 0.0 || image.y <= 0.0 || avail.x <= 0.0 || avail.y <= 0.0 {
        return 1.0;
    }
    let scale = (avail.x / image.x).min(avail.y / image.y);
    if scale >= 1.0 { scale.floor().min(MAX_ZOOM) } else { scale.max(MIN_ZOOM) }
}

pub fn step_in(zoom: f32) -> f32 {
    STEPS.iter().copied().find(|&s| s > zoom * 1.001).unwrap_or(MAX_ZOOM)
}

pub fn step_out(zoom: f32) -> f32 {
    STEPS.iter().rev().copied().find(|&s| s < zoom * 0.999).unwrap_or(MIN_ZOOM)
}

impl View {
    /// Re-apply fit mode and keep the image from drifting out of view.
    pub fn settle(&mut self, image: Vec2, canvas: Vec2, padding: f32) {
        if self.fit {
            self.zoom = fit_zoom(image, canvas - Vec2::splat(padding * 2.0));
            self.pan = Vec2::ZERO;
        }
        self.pan = clamp_pan(self.pan, image * self.zoom, canvas);
    }

    /// Change zoom keeping the image point under `anchor` (relative to canvas centre) still.
    pub fn zoom_to(&mut self, zoom: f32, anchor: Vec2, image: Vec2, canvas: Vec2) {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        self.pan = anchor - (anchor - self.pan) * ratio;
        self.zoom = zoom;
        self.fit = false;
        self.pan = clamp_pan(self.pan, image * zoom, canvas);
    }

    pub fn pan_by(&mut self, delta: Vec2, image: Vec2, canvas: Vec2) {
        self.pan = clamp_pan(self.pan + delta, image * self.zoom, canvas);
    }

    /// Where the image lands, snapped so every image pixel covers whole screen pixels.
    pub fn image_rect(&self, canvas: Rect, image: Vec2) -> Rect {
        let size = image * self.zoom;
        let centre = canvas.center() + self.pan;
        let min = pos2((centre.x - size.x / 2.0).round(), (centre.y - size.y / 2.0).round());
        Rect::from_min_size(min, size)
    }

    /// Image pixel under a screen position, if any.
    pub fn pixel_at(&self, canvas: Rect, image: Vec2, screen: Pos2) -> Option<[u32; 2]> {
        let rect = self.image_rect(canvas, image);
        if !rect.contains(screen) {
            return None;
        }
        let p = (screen - rect.min) / self.zoom;
        Some([(p.x as u32).min(image.x as u32 - 1), (p.y as u32).min(image.y as u32 - 1)])
    }
}

/// A view part-way from `a` to `b`: zoom moves evenly in log space (each doubling takes the
/// same time), pan linearly. `t` is 0..=1.
pub fn blend(a: View, b: View, t: f32) -> View {
    let t = t.clamp(0.0, 1.0);
    let zoom = (a.zoom.ln() + (b.zoom.ln() - a.zoom.ln()) * t).exp();
    View { fit: b.fit, zoom, pan: a.pan + (b.pan - a.pan) * t }
}

/// Fast start, gentle landing.
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// An image smaller than the canvas stays centred; a larger one can't leave a gap at an edge.
pub fn clamp_pan(pan: Vec2, shown: Vec2, canvas: Vec2) -> Vec2 {
    let axis = |p: f32, shown: f32, canvas: f32| {
        let slack = ((shown - canvas) / 2.0).max(0.0);
        p.clamp(-slack, slack)
    };
    vec2(axis(pan.x, shown.x, canvas.x), axis(pan.y, shown.y, canvas.y))
}

pub fn percent(zoom: f32) -> String {
    let p = zoom * 100.0;
    if p >= 10.0 || (p - p.round()).abs() < 0.05 { format!("{:.0}%", p) } else { format!("{:.1}%", p) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_images_fit_at_whole_multiples() {
        assert_eq!(fit_zoom(vec2(64.0, 64.0), vec2(300.0, 280.0)), 4.0);
        assert_eq!(fit_zoom(vec2(64.0, 64.0), vec2(127.0, 1000.0)), 1.0);
        assert_eq!(fit_zoom(vec2(1.0, 1.0), vec2(5000.0, 5000.0)), MAX_ZOOM);
    }

    #[test]
    fn large_images_shrink_smoothly() {
        let z = fit_zoom(vec2(4000.0, 3000.0), vec2(1000.0, 1000.0));
        assert!((z - 0.25).abs() < 1e-6);
    }

    #[test]
    fn steps_are_integers_above_100_percent() {
        assert_eq!(step_in(1.0), 2.0);
        assert_eq!(step_in(2.0), 3.0);
        assert_eq!(step_in(0.3), 1.0 / 3.0);
        assert_eq!(step_out(1.0), 2.0 / 3.0);
        assert_eq!(step_out(3.0), 2.0);
        // A fractional fit zoom steps to the neighbouring stops.
        assert_eq!(step_in(0.27), 1.0 / 3.0);
        assert_eq!(step_out(0.27), 0.25);
        assert_eq!(step_in(MAX_ZOOM), MAX_ZOOM);
        assert_eq!(step_out(MIN_ZOOM), MIN_ZOOM);
    }

    #[test]
    fn zoom_keeps_the_pixel_under_the_cursor() {
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let image = vec2(400.0, 300.0);
        let mut view = View { fit: false, zoom: 1.0, pan: Vec2::ZERO };
        let cursor = pos2(500.0, 350.0);
        let before = view.pixel_at(canvas, image, cursor).unwrap();
        view.zoom_to(4.0, cursor - canvas.center(), image, canvas.size());
        let after = view.pixel_at(canvas, image, cursor).unwrap();
        assert_eq!(before, after);
        assert!(!view.fit);
    }

    #[test]
    fn image_corners_land_on_whole_screen_pixels() {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0));
        let view = View { fit: false, zoom: 3.0, pan: vec2(0.3, -0.7) };
        let r = view.image_rect(canvas, vec2(33.0, 17.0));
        assert_eq!(r.min.x.fract(), 0.0);
        assert_eq!(r.min.y.fract(), 0.0);
        assert_eq!(r.size(), vec2(99.0, 51.0));
    }

    #[test]
    fn panning_stops_at_the_edges() {
        let shown = vec2(1000.0, 200.0);
        let canvas = vec2(600.0, 400.0);
        assert_eq!(clamp_pan(vec2(900.0, 50.0), shown, canvas), vec2(200.0, 0.0));
        assert_eq!(clamp_pan(vec2(-900.0, -50.0), shown, canvas), vec2(-200.0, 0.0));
    }

    #[test]
    fn fit_mode_follows_the_window() {
        let mut view = View::default();
        view.settle(vec2(64.0, 64.0), vec2(400.0, 400.0), 0.0);
        assert_eq!(view.zoom, 6.0);
        view.settle(vec2(64.0, 64.0), vec2(200.0, 200.0), 0.0);
        assert_eq!(view.zoom, 3.0);
    }

    #[test]
    fn blending_is_even_in_log_zoom() {
        let a = View { fit: false, zoom: 1.0, pan: Vec2::ZERO };
        let b = View { fit: false, zoom: 4.0, pan: vec2(10.0, 0.0) };
        let mid = blend(a, b, 0.5);
        assert!((mid.zoom - 2.0).abs() < 1e-5);
        assert_eq!(mid.pan, vec2(5.0, 0.0));
        assert_eq!(blend(a, b, 1.0).zoom, 4.0);
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
    }

    #[test]
    fn percent_labels() {
        assert_eq!(percent(4.0), "400%");
        assert_eq!(percent(1.0 / 3.0), "33%");
        assert_eq!(percent(1.0 / 12.0), "8.3%");
    }
}
