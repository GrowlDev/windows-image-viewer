//! The side gallery: every image in the folder as a card. Click one to view it.

use std::path::{Path, PathBuf};

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextFormat, pos2, vec2,
};

use crate::thumbs::{Slot, Thumbs};
use crate::{folder, theme};

pub const MIN_CARD: f32 = 64.0;
pub const MAX_CARD: f32 = 256.0;
const GAP: f32 = 8.0;
const LABEL: f32 = 20.0;

pub struct Gallery {
    pub open: bool,
    /// Preferred card width in points; cards stretch slightly to fill the row.
    pub card: f32,
    thumbs: Thumbs,
    /// The file last scrolled into view, so browsing with the arrow keys follows along.
    followed: Option<PathBuf>,
    viewport: Rect,
}

impl Gallery {
    pub fn new(ctx: egui::Context, open: bool, card: f32) -> Self {
        Self {
            open,
            card: card.clamp(MIN_CARD, MAX_CARD),
            thumbs: Thumbs::new(ctx),
            followed: None,
            viewport: Rect::NOTHING,
        }
    }

    pub fn poll(&mut self) {
        self.thumbs.poll();
    }

    pub fn invalidate(&mut self, path: &Path) {
        self.thumbs.invalidate(path);
    }

    /// Draw the gallery; returns the file the user clicked.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        files: &[PathBuf],
        current: Option<&Path>,
        checker: egui::TextureId,
    ) -> Option<PathBuf> {
        let mut clicked = None;
        let ppp = ui.ctx().pixels_per_point();

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let folder_name = current
                .or(files.first().map(PathBuf::as_path))
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.label(RichText::new(folder_name).strong());
            let count = if files.len() == 1 { "1 image".to_owned() } else { format!("{} images", files.len()) };
            ui.label(RichText::new(count).color(theme::TEXT_DIM));
        });
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Size").color(theme::TEXT_DIM).small());
            ui.spacing_mut().slider_width = (ui.available_width() - 24.0).max(40.0);
            ui.add(egui::Slider::new(&mut self.card, MIN_CARD..=MAX_CARD).show_value(false))
                .on_hover_text("Card size  (Ctrl+wheel over the gallery)");
        });
        ui.add_space(6.0);

        // Ctrl+wheel over the gallery resizes the cards.
        if ui.rect_contains_pointer(ui.max_rect()) {
            // egui turns Ctrl+wheel into a zoom gesture.
            let zoom = ui.input(|i| i.zoom_delta());
            if zoom != 1.0 {
                self.card = (self.card * zoom).clamp(MIN_CARD, MAX_CARD);
            }
        }

        let width = ui.available_width() - 2.0 * 12.0;
        let columns = (((width + GAP) / (self.card + GAP)).floor() as usize).max(1);
        let card_w = ((width - GAP * (columns - 1) as f32) / columns as f32).floor();
        let row_h = card_w + LABEL + GAP;
        let rows = files.len().div_ceil(columns);

        let mut area = egui::ScrollArea::vertical().id_salt("gallery").auto_shrink([false, false]);
        if self.followed.as_deref() != current {
            self.followed = current.map(Path::to_path_buf);
            if let Some(i) = current.and_then(|c| folder::position(files, c)) {
                let top = (i / columns) as f32 * row_h;
                let visible =
                    self.viewport.is_positive() && top >= self.viewport.top() && top + row_h <= self.viewport.bottom();
                if !visible {
                    let height = if self.viewport.is_positive() { self.viewport.height() } else { 600.0 };
                    area = area.vertical_scroll_offset((top - (height - row_h) / 2.0).max(0.0));
                }
            }
        }

        let mut shown = Vec::new();
        area.show_viewport(ui, |ui, viewport| {
            self.viewport = viewport;
            ui.set_height(rows as f32 * row_h);
            let origin = ui.max_rect().min + vec2(12.0, 0.0);
            let first = (viewport.top() / row_h).floor().max(0.0) as usize;
            let last = ((viewport.bottom() / row_h).ceil() as usize + 1).min(rows);
            for row in first..last {
                for col in 0..columns {
                    let i = row * columns + col;
                    let Some(path) = files.get(i) else { break };
                    let min = origin + vec2(col as f32 * (card_w + GAP), row as f32 * row_h);
                    let rect = Rect::from_min_size(min, vec2(card_w, card_w + LABEL));
                    let is_current = current == Some(path.as_path());
                    if self.card_ui(ui, rect, path, is_current, ppp, checker) {
                        clicked = Some(path.clone());
                    }
                    shown.push(path.clone());
                }
            }
        });
        self.thumbs.show(&shown);
        clicked
    }

    fn card_ui(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        path: &Path,
        is_current: bool,
        ppp: f32,
        checker: egui::TextureId,
    ) -> bool {
        let response = ui.interact(rect, ui.id().with(path), Sense::click());
        let painter = ui.painter();
        let tile = Rect::from_min_size(rect.min, vec2(rect.width(), rect.width()));
        let fill = if response.hovered() { theme::BUTTON_HOVER } else { theme::CANVAS };
        painter.rect_filled(tile, CornerRadius::same(6), fill);

        match self.thumbs.get(path) {
            Some(Slot::Ready(t)) => {
                let shown = fit_thumb(tile.shrink(6.0), [t.width, t.height], t.texture.size(), ppp);
                if t.has_alpha {
                    let cell = (6.0 * ppp).round().max(3.0);
                    let uv = Rect::from_min_max(Pos2::ZERO, (shown.size() * ppp / (cell * 2.0)).to_pos2());
                    painter.image(checker, shown, uv, Color32::WHITE);
                }
                painter.image(t.texture.id(), shown, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
            }
            Some(Slot::Failed) => {
                painter.text(
                    tile.center(),
                    egui::Align2::CENTER_CENTER,
                    "?",
                    FontId::proportional(20.0),
                    theme::TEXT_DISABLED,
                );
            }
            _ => {}
        }
        if is_current {
            painter.rect_stroke(tile, CornerRadius::same(6), Stroke::new(2.0, theme::ACCENT), StrokeKind::Inside);
        }

        let name = folder::file_name(path);
        let mut job = LayoutJob::single_section(
            name.to_owned(),
            TextFormat::simple(FontId::proportional(12.0), if is_current { theme::TEXT } else { theme::TEXT_DIM }),
        );
        job.wrap =
            TextWrapping {
                max_width: rect.width(), max_rows: 1, break_anywhere: true, overflow_character: Some('…')
            };
        job.halign = Align::Center;
        let galley = painter.layout_job(job);
        painter.galley(pos2(rect.center().x, tile.bottom() + 3.0), galley, theme::TEXT_DIM);

        let response = response.on_hover_text(match self.thumbs.get(path) {
            Some(Slot::Ready(t)) => format!("{name}\n{} × {}", t.width, t.height),
            _ => name.to_owned(),
        });
        response.clicked()
    }
}

/// Where a thumbnail sits in its tile. Small images are enlarged by a whole number of screen
/// pixels (crisp pixel art); larger ones are fitted.
fn fit_thumb(area: Rect, [w, h]: [u32; 2], texture: [usize; 2], ppp: f32) -> Rect {
    let (w, h) = (w as f32, h as f32);
    let area_px = area.size() * ppp;
    let size_px = if texture[0] as f32 >= w && w.max(h) <= area_px.x.min(area_px.y) {
        let scale = (area_px.x / w).min(area_px.y / h).floor().max(1.0);
        vec2(w, h) * scale
    } else {
        let scale = (area_px.x / w).min(area_px.y / h);
        vec2(w, h) * scale
    };
    let centre = area.center() * ppp;
    let min = pos2((centre.x - size_px.x / 2.0).round(), (centre.y - size_px.y / 2.0).round());
    Rect::from_min_size((min.to_vec2() / ppp).to_pos2(), size_px / ppp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_sprites_enlarge_by_whole_pixels() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let r = fit_thumb(area, [16, 16], [16, 16], 1.0);
        assert_eq!(r.size(), vec2(96.0, 96.0));
        assert_eq!((r.min.x.fract(), r.min.y.fract()), (0.0, 0.0));
    }

    #[test]
    fn photos_fit_inside() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let r = fit_thumb(area, [4000, 3000], [256, 192], 1.0);
        assert!((r.width() - 100.0).abs() < 0.01 && (r.height() - 75.0).abs() < 0.01);
    }
}
