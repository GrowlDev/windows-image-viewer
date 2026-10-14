//! The bars around the canvas: file info, toolbar, toasts, window title.

use super::*;

impl ViewerApp {
    /// While files are dragged over the window, show where they'll go.
    pub(super) fn drop_overlay(&self, ctx: &egui::Context, area: Rect) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
        let rect = area.shrink(14.0);
        painter.rect_filled(rect, CornerRadius::same(10), theme::WINDOW.gamma_multiply(0.88));
        painter.rect_stroke(rect, CornerRadius::same(10), Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drop to open",
            FontId::proportional(20.0),
            theme::TEXT,
        );
    }

    pub(super) fn info_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            let Some(path) = self.current.clone() else {
                ui.label(RichText::new(register::APP_NAME).color(theme::TEXT_DIM));
                return;
            };
            ui.label(RichText::new(folder::file_name(&path)).strong().color(theme::TEXT));
            let details = match self.loader.get(&path) {
                Some(Slot::Ready(l)) => {
                    let i = &l.info;
                    let mut parts = vec![
                        format!("{} × {}", i.width, i.height),
                        i.format.to_string(),
                        platform::file_size(i.file_size),
                    ];
                    if i.is_animated() {
                        parts.push(format!("{} frames", i.frames.len()));
                    }
                    if let Some(t) = i.modified.and_then(platform::local_time) {
                        parts.push(t);
                    }
                    RichText::new(parts.join("  ·  ")).color(theme::TEXT_DIM)
                }
                Some(Slot::Failed(e)) => RichText::new(e).color(theme::DANGER),
                _ => RichText::new("Loading…").color(theme::TEXT_DIM),
            };
            ui.add_space(8.0);
            ui.label(details);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if let Some(i) = self.index() {
                    ui.label(RichText::new(format!("{} / {}", i + 1, self.files.len())).color(theme::TEXT_DIM));
                }
            });
        });
    }

    pub(super) fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        image: Option<Vec2>,
        canvas: Rect,
        ppp: f32,
    ) {
        let has_image = image.is_some();
        let has_file = self.current.is_some();
        let many = self.files.len() > 1;
        let canvas_size = canvas.size() * ppp;
        // Narrow windows: the Edit button drops its label, then the pixel readout goes.
        let width = ui.available_width();
        let (compact, tight) = (width < 1000.0, width < 840.0);
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            if icons::button(ui, Icon::Gallery, "Gallery  (G)", true, self.gallery.open) {
                self.gallery.open = !self.gallery.open;
            }
            divider(ui);
            if icons::button(ui, Icon::Prev, "Previous  (←)", many, false) {
                self.go(-1);
            }
            if icons::button(ui, Icon::Next, "Next  (→)", many, false) {
                self.go(1);
            }
            divider(ui);
            if icons::button(ui, Icon::ZoomOut, "Zoom out  (−, wheel)", has_image, false)
                && let Some(img) = image
            {
                self.set_zoom(view::step_out(self.view.zoom), Vec2::ZERO, img, canvas_size);
            }
            let label = RichText::new(view::percent(self.view.zoom)).font(FontId::proportional(14.0));
            let pct = ui.add_enabled(has_image, egui::Button::new(label).min_size(vec2(58.0, 30.0)).frame(false));
            if pct.on_hover_text("100%  (1)  ·  click to toggle Fit  (0)").clicked() {
                if self.view.zoom == 1.0 && !self.view.fit {
                    self.fit_view();
                } else if let Some(img) = image {
                    self.set_zoom(1.0, Vec2::ZERO, img, canvas_size);
                }
            }
            if icons::button(ui, Icon::ZoomIn, "Zoom in  (+, wheel)", has_image, false)
                && let Some(img) = image
            {
                self.set_zoom(view::step_in(self.view.zoom), Vec2::ZERO, img, canvas_size);
            }
            if icons::button(ui, Icon::Fit, "Fit to window  (0)", has_image, self.view.fit) {
                self.fit_view();
            }
            divider(ui);
            if icons::button(ui, Icon::RotateLeft, "Rotate left  (Ctrl+,)", has_image, false) {
                self.rotate(3);
            }
            if icons::button(ui, Icon::RotateRight, "Rotate right  (Ctrl+.)", has_image, false) {
                self.rotate(1);
            }

            if let Some(l) = self.loaded().filter(|l| l.info.is_animated()) {
                divider(ui);
                let (icon, tip) =
                    if self.anim.playing { (Icon::Pause, "Pause  (Space)") } else { (Icon::Play, "Play  (Space)") };
                if icons::button(ui, icon, tip, true, false) {
                    self.toggle_play();
                }
                let n = l.info.frames.len();
                let frame_label = RichText::new(format!("{:>w$} / {n}", self.anim.frame + 1, w = n.to_string().len()))
                    .color(theme::TEXT_DIM)
                    .monospace();
                ui.label(frame_label).on_hover_text("Step frames with , and .");
                if icons::button(
                    ui,
                    Icon::Loop,
                    if self.looping { "Loop is on  (L)" } else { "Loop is off  (L)" },
                    true,
                    self.looping,
                ) {
                    self.toggle_loop();
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if icons::button(ui, Icon::Settings, "Settings", true, self.settings_open) {
                    self.open_settings();
                }
                let label = format!("Edit in {}", self.editor.name);
                let tip = format!(
                    "Open in {}  (E)\nSaved changes show here automatically.\nChange the editor in Settings.",
                    self.editor.name
                );
                let clicked = if compact {
                    icons::button(ui, Icon::Edit, &format!("{label}  (E)"), has_file, false)
                } else {
                    icons::text_button(ui, Icon::Edit, &label, &tip, has_file)
                };
                if clicked {
                    self.edit();
                }
                ui.add_space(6.0);
                if icons::button(ui, Icon::Delete, "Move to Recycle Bin  (Del)", has_file, false) {
                    self.delete();
                }
                if icons::button(ui, Icon::OpenWith, "Open with…", has_file, false)
                    && let (Some(hwnd), Some(p)) = (self.hwnd, self.current.clone())
                {
                    platform::open_with(hwnd as _, &p);
                }
                if icons::button(ui, Icon::Folder, "Show in Explorer", has_file, false)
                    && let Some(p) = self.current.clone()
                    && let Err(e) = platform::show_in_explorer(&p)
                {
                    self.notify(e, true);
                }
                if icons::button(ui, Icon::Copy, "Copy image  (Ctrl+C)", has_image, false) {
                    self.copy();
                }
                if icons::button(ui, Icon::Info, "File info bar  (I)", true, self.show_info) {
                    self.show_info = !self.show_info;
                }
                if !tight {
                    divider(ui);
                    self.pixel_readout(ui);
                }
            });
        });
        let _ = ctx;
    }

    /// Coordinates and colour of the pixel under the cursor.
    pub(super) fn pixel_readout(&self, ui: &mut egui::Ui) {
        let Some(([x, y], color)) = self.hover_pixel else { return };
        match color.map(|c| c.to_array()) {
            Some([_, _, _, 0]) => {
                ui.label(RichText::new("transparent").monospace().color(theme::TEXT_DIM));
            }
            Some([r, g, b, a]) => {
                let hex = format!("#{r:02X}{g:02X}{b:02X}");
                let text = if a == 255 { hex } else { format!("{hex} {}%", (a as u32 * 100 + 127) / 255) };
                ui.label(RichText::new(text).monospace().color(theme::TEXT_DIM));
                let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
                ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, theme::DIVIDER), egui::StrokeKind::Outside);
            }
            None => {}
        }
        ui.add_space(4.0);
        ui.label(RichText::new(format!("{x}, {y}")).monospace().color(theme::TEXT_DIM));
    }

    pub(super) fn draw_toast(&mut self, ctx: &egui::Context, canvas: Rect) {
        let Some((text, error, at)) = &self.toast else { return };
        let age = at.elapsed().as_secs_f32();
        const SHOW: f32 = 2.2;
        if age > SHOW {
            self.toast = None;
            return;
        }
        let alpha = ((SHOW - age) / 0.3).clamp(0.0, 1.0);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("toast")));
        let color = if *error { theme::DANGER } else { theme::TEXT };
        let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(14.0), color.gamma_multiply(alpha));
        let size = galley.size() + vec2(28.0, 16.0);
        let rect = Rect::from_center_size(pos2(canvas.center().x, canvas.bottom() - 20.0 - size.y / 2.0), size);
        painter.rect_filled(
            rect,
            CornerRadius::same(6),
            Color32::from_rgba_unmultiplied(16, 18, 21, (235.0 * alpha) as u8),
        );
        painter.galley(rect.min + vec2(14.0, 8.0), galley, color);
        ctx.request_repaint();
    }

    pub(super) fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.current {
            Some(p) => format!("{} - {}", folder::file_name(p), register::APP_NAME),
            None => register::APP_NAME.to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::DIVIDER);
    ui.add_space(4.0);
}
