//! The image canvas: placement, checkerboard, rotation, pixel readout.

use super::*;

/// Rounded corners of the windowed canvas, in points.
const CANVAS_RADIUS: f32 = 8.0;

impl ViewerApp {
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, fullscreen: bool, ppp: f32) {
        let painter = ui.painter_at(rect);
        if fullscreen {
            painter.rect_filled(rect, 0.0, theme::pal().fullscreen);
        } else {
            painter.rect_filled(rect, CornerRadius::same(CANVAS_RADIUS as u8), theme::pal().canvas);
        }
        let response = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
        self.hover_pixel = None;

        let Some(path) = self.current.clone() else {
            self.empty_state(ui, rect);
            return;
        };
        if response.double_clicked() {
            self.toggle_fullscreen(ui.ctx());
        }
        let canvas_px = Rect::from_min_max(
            pos2((rect.min.x * ppp).round(), (rect.min.y * ppp).round()),
            pos2((rect.max.x * ppp).round(), (rect.max.y * ppp).round()),
        );
        let loaded = match self.loader.get(&path) {
            Some(Slot::Ready(l)) => l.clone(),
            Some(Slot::Failed(e)) => {
                centered_text(
                    &painter,
                    rect,
                    &format!("Can't show {}\n{e}", folder::file_name(&path)),
                    theme::pal().danger,
                );
                return;
            }
            _ => {
                // Keep the previous picture up while this one decodes: most loads finish
                // within a frame or two, and a blank canvas in between reads as a flicker.
                if let Some(prev) = &self.last_shown {
                    draw_image(&painter, prev, canvas_px, ppp, self.checker.id());
                }
                let waited = self.current_since.elapsed();
                if waited > LOADING_GRACE {
                    loading_pill(&painter, rect);
                } else {
                    ui.ctx().request_repaint_after(LOADING_GRACE - waited);
                }
                if !fullscreen {
                    mask_corners(&painter, rect, CANVAS_RADIUS, theme::pal().window);
                }
                return;
            }
        };

        let rotation = self.rotation();
        let image = display_size(&loaded, rotation);
        self.view.settle(image, canvas_px.size(), (PADDING * ppp).round());

        if response.dragged() {
            // Panning follows the hand exactly; it never eases.
            self.view_from = None;
            self.view.pan_by(response.drag_delta() * ppp, image, canvas_px.size());
        }
        let pannable =
            image.x * self.view.zoom > canvas_px.width() + 0.5 || image.y * self.view.zoom > canvas_px.height() + 0.5;
        if response.dragged() && pannable {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() && pannable {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }

        let view = self.shown_view();
        if let Some((_, started)) = self.view_from {
            if started.elapsed() >= VIEW_ANIMATION {
                self.view_from = None;
            } else {
                ui.ctx().request_repaint();
            }
        }
        let shown = Shown { loaded: loaded.clone(), view, rotation };
        draw_image(&painter, &shown, canvas_px, ppp, self.checker.id());
        self.last_shown = Some(shown);

        if let Some(pointer) = response.hover_pos() {
            let p = pos2(pointer.x * ppp, pointer.y * ppp);
            if let Some(px) = view.pixel_at(canvas_px, image, p) {
                let [w, h] = [loaded.info.width, loaded.info.height];
                let src = unrotate(px, rotation, w, h);
                self.hover_pixel = Some((src, pixel_color(&loaded, self.anim.frame, src)));
            }
        }
        if !fullscreen {
            mask_corners(&painter, rect, CANVAS_RADIUS, theme::pal().window);
        }
    }

    /// No image yet: the app's icon, an Open button, and how else to get started.
    fn empty_state(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = rect.center();
        let painter = ui.painter_at(rect);
        let icon = Rect::from_center_size(c + vec2(0.0, -78.0), vec2(96.0, 96.0));
        painter.image(self.app_icon.id(), icon, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        painter.text(
            c + vec2(0.0, -6.0),
            egui::Align2::CENTER_CENTER,
            register::APP_NAME,
            FontId::proportional(20.0),
            theme::pal().text,
        );
        let button = egui::Button::new(RichText::new("Open an image…").size(15.0))
            .fill(theme::pal().raised)
            .stroke(Stroke::new(1.0, theme::pal().divider))
            .corner_radius(6.0)
            .min_size(vec2(170.0, 34.0));
        if ui.put(Rect::from_center_size(c + vec2(0.0, 40.0), vec2(170.0, 34.0)), button).clicked() {
            self.open_dialog();
        }
        painter.text(
            c + vec2(0.0, 82.0),
            egui::Align2::CENTER_CENTER,
            "or drop one here  ·  Ctrl+O  ·  F1 for keyboard shortcuts",
            FontId::proportional(13.0),
            theme::pal().text_dim,
        );
    }
}

/// Draw an image as placed by `shown.view`, on a checkerboard if it has transparency.
fn draw_image(painter: &egui::Painter, shown: &Shown, canvas_px: Rect, ppp: f32, checker: egui::TextureId) {
    let image = display_size(&shown.loaded, shown.rotation);
    let px = shown.view.image_rect(canvas_px, image);
    let rect = Rect::from_min_max((px.min.to_vec2() / ppp).to_pos2(), (px.max.to_vec2() / ppp).to_pos2());
    if shown.loaded.info.has_alpha {
        let cell = (8.0 * ppp).round().max(4.0);
        let uv = Rect::from_min_max(Pos2::ZERO, (px.size() / (cell * 2.0)).to_pos2());
        painter.image(checker, rect, uv, Color32::WHITE);
    }
    painter.add(Shape::mesh(rotated_quad(shown.loaded.texture.id(), rect, shown.rotation)));
}

/// "Loading…", for the rare image that takes longer than a moment.
fn loading_pill(painter: &egui::Painter, canvas: Rect) {
    let galley = painter.layout_no_wrap("Loading…".into(), FontId::proportional(13.0), theme::HUD_TEXT);
    let size = galley.size() + vec2(24.0, 12.0);
    let rect = Rect::from_center_size(pos2(canvas.center().x, canvas.bottom() - 24.0 - size.y / 2.0), size);
    painter.rect_filled(rect, CornerRadius::same(6), theme::HUD.gamma_multiply(0.9));
    painter.galley(rect.min + vec2(12.0, 6.0), galley, theme::HUD_TEXT);
}

/// Paint the window colour over the canvas's corners so a large image follows the rounding.
fn mask_corners(painter: &egui::Painter, rect: Rect, radius: f32, color: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let r = radius;
    let corners = [
        (rect.left_top(), rect.left_top() + vec2(r, r), PI),
        (rect.right_top(), rect.right_top() + vec2(-r, r), 1.5 * PI),
        (rect.right_bottom(), rect.right_bottom() + vec2(-r, -r), 0.0),
        (rect.left_bottom(), rect.left_bottom() + vec2(r, -r), FRAC_PI_2),
    ];
    for (corner, centre, start) in corners {
        // A fan from the corner point to the quarter arc covers exactly what lies outside it.
        let mut mesh = Mesh::default();
        mesh.colored_vertex(corner, color);
        const STEPS: u32 = 8;
        for i in 0..=STEPS {
            let a = start + FRAC_PI_2 * i as f32 / STEPS as f32;
            mesh.colored_vertex(centre + vec2(a.cos(), a.sin()) * r, color);
        }
        for i in 1..=STEPS {
            mesh.add_triangle(0, i, i + 1);
        }
        painter.add(Shape::mesh(mesh));
    }
}

pub(super) fn centered_text(painter: &egui::Painter, rect: Rect, text: &str, color: Color32) {
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(15.0), color);
}

pub(super) fn display_size(loaded: &Loaded, rotation: u8) -> Vec2 {
    let (w, h) = (loaded.info.width as f32, loaded.info.height as f32);
    if rotation % 2 == 1 { vec2(h, w) } else { vec2(w, h) }
}

/// A textured quad whose texture is turned `rotation` quarter turns clockwise.
fn rotated_quad(texture: egui::TextureId, rect: Rect, rotation: u8) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    mesh.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    // Vertex order is TL, TR, BL, BR; walk the corners clockwise to rotate.
    let corners = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
    let clockwise_slot = [0, 1, 3, 2];
    for (i, &slot) in clockwise_slot.iter().enumerate() {
        mesh.vertices[slot].uv = corners[(i + 4 - rotation as usize % 4) % 4];
    }
    mesh
}

/// Display pixel -> source pixel, undoing the view rotation.
fn unrotate([x, y]: [u32; 2], rotation: u8, w: u32, h: u32) -> [u32; 2] {
    match rotation % 4 {
        1 => [y, h - 1 - x],
        2 => [w - 1 - x, h - 1 - y],
        3 => [w - 1 - y, x],
        _ => [x, y],
    }
}

fn pixel_color(loaded: &Loaded, frame: usize, [x, y]: [u32; 2]) -> Option<Color32> {
    let f = loaded.info.frames.get(frame.min(loaded.info.frames.len().saturating_sub(1)))?;
    let [w, h] = f.image.size;
    if [w, h] != [loaded.info.width as usize, loaded.info.height as usize] {
        return None;
    }
    let c = f.image.pixels.get(y as usize * w + x as usize)?;
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(super) fn unrotate_round_trips_corners() {
        let (w, h) = (4, 2);
        // Rotated 90° clockwise, the display is 2 wide and 4 tall; its top-left shows
        // the source's bottom-left.
        assert_eq!(unrotate([0, 0], 1, w, h), [0, 1]);
        assert_eq!(unrotate([1, 0], 1, w, h), [0, 0]);
        assert_eq!(unrotate([0, 3], 1, w, h), [3, 1]);
        assert_eq!(unrotate([0, 0], 2, w, h), [3, 1]);
        assert_eq!(unrotate([0, 0], 3, w, h), [3, 0]);
    }

    #[test]
    pub(super) fn quad_rotation_matches_unrotate() {
        let rect = Rect::from_min_max(Pos2::ZERO, pos2(2.0, 4.0));
        let mesh = rotated_quad(egui::TextureId::default(), rect, 1);
        // Display top-left samples the texture's bottom-left.
        assert_eq!(mesh.vertices[0].uv, pos2(0.0, 1.0));
        // Display top-right samples the texture's top-left.
        assert_eq!(mesh.vertices[1].uv, pos2(0.0, 0.0));
    }
}
