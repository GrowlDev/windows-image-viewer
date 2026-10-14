//! Off-screen test harness: scripted actions and screenshots (see README).

use super::*;

impl ViewerApp {
    /// Test harness: one scripted action per settled frame, then a screenshot.
    pub(super) fn run_script(&mut self, ctx: &egui::Context, image: Option<Vec2>, canvas: Rect, ppp: f32) {
        if self.capture.is_none() && self.script.is_empty() {
            return;
        }
        ctx.request_repaint();
        let loading = self.current.as_ref().is_some_and(|p| matches!(self.loader.get(p), Some(Slot::Loading) | None));
        self.frames_since_action += 1;
        if loading || self.frames_since_action < 3 {
            return;
        }
        self.frames_since_action = 0;
        if let Some(action) = self.script.pop() {
            let size = canvas.size() * ppp;
            let zoom = |app: &mut Self, z: f32| {
                if let Some(img) = image {
                    app.set_zoom(z, Vec2::ZERO, img, size);
                }
            };
            match action.as_str() {
                "next" => self.go(1),
                "prev" => self.go(-1),
                "zoomin" => zoom(self, view::step_in(self.view.zoom)),
                "zoomout" => zoom(self, view::step_out(self.view.zoom)),
                "actual" => zoom(self, 1.0),
                "fit" => self.fit_view(),
                "rotate" => self.rotate(1),
                "pause" => self.toggle_play(),
                "step" => self.step_frame(1),
                "info" => self.show_info = !self.show_info,
                "fullscreen" => self.toggle_fullscreen(ctx),
                "gallery" => self.gallery.open = !self.gallery.open,
                "settings" => self.open_settings(),
                "shortcuts" => self.shortcuts_open = true,
                "dark" => self.set_theme(ctx, theme::Choice::Dark),
                "light" => self.set_theme(ctx, theme::Choice::Light),
                "wait" => {}
                other => {
                    if let Some(z) = other.strip_prefix("zoom").and_then(|z| z.parse::<f32>().ok()) {
                        zoom(self, z);
                    }
                }
            }
            return;
        }
        if self.capture.is_some() && !self.shot_requested {
            self.shot_requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    /// The screenshot arrives as an input event on the frame after it was requested.
    pub(super) fn receive_capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.capture.clone() else { return };
        for event in ctx.input(|i| i.events.clone()) {
            if let Event::Screenshot { image, .. } = event {
                crate::trace("screenshot received");
                let [w, h] = image.size;
                // The window is opaque on screen; drop the framebuffer's leftover alpha.
                let rgba: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|c| {
                        let [r, g, b, _] = c.to_array();
                        [r, g, b, 255]
                    })
                    .collect();
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                    let _ = img.save(&path);
                }
                self.capture = None;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}
