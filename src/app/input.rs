//! Keyboard, mouse-button and wheel/pinch input.

use super::*;

impl ViewerApp {
    pub(super) fn handle_keys(&mut self, ctx: &egui::Context, image: Option<Vec2>, canvas: Rect, ppp: f32) {
        let events = ctx.input(|i| i.events.clone());
        let animated = self.loaded().is_some_and(|l| l.info.is_animated());
        let canvas_size = canvas.size() * ppp;
        for event in events {
            match event {
                Event::Copy => self.copy(),
                Event::Key { key, pressed: true, modifiers, .. } => {
                    let ctrl = modifiers.command;
                    let zoom_to = |app: &mut Self, z: f32| {
                        if let Some(img) = image {
                            app.set_zoom(z, Vec2::ZERO, img, canvas_size);
                        }
                    };
                    match key {
                        Key::ArrowRight | Key::PageDown => self.go(1),
                        Key::ArrowLeft | Key::PageUp => self.go(-1),
                        Key::Home => self.go_to(0),
                        Key::End => self.go_to(self.files.len().saturating_sub(1)),
                        Key::Plus | Key::Equals => zoom_to(self, view::step_in(self.view.zoom)),
                        Key::Minus => zoom_to(self, view::step_out(self.view.zoom)),
                        Key::Num0 => self.fit_view(),
                        Key::Num1 if !ctrl => zoom_to(self, 1.0),
                        Key::Num2
                        | Key::Num3
                        | Key::Num4
                        | Key::Num5
                        | Key::Num6
                        | Key::Num7
                        | Key::Num8
                        | Key::Num9
                            if !ctrl =>
                        {
                            let n = key.name().parse::<f32>().unwrap_or(1.0);
                            zoom_to(self, n);
                        }
                        Key::Period if ctrl => self.rotate(1),
                        Key::Comma if ctrl => self.rotate(3),
                        Key::R if modifiers.shift => self.rotate(3),
                        Key::R => self.rotate(1),
                        Key::Period if animated => self.step_frame(1),
                        Key::Comma if animated => self.step_frame(-1),
                        Key::Space if animated => self.toggle_play(),
                        Key::L => self.toggle_loop(),
                        Key::I => self.show_info = !self.show_info,
                        Key::G => self.gallery.open = !self.gallery.open,
                        Key::E => self.edit(),
                        Key::F1 | Key::Questionmark => self.shortcuts_open = true,
                        Key::O if ctrl => self.open_dialog(),
                        Key::Delete => self.delete(),
                        Key::F5 => {
                            if let Some(p) = self.current.clone() {
                                self.rescan_folder_of(&p);
                                self.loader.reload(&p);
                            }
                        }
                        Key::F11 | Key::Enter => self.toggle_fullscreen(ctx),
                        Key::Escape => {
                            if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                            } else {
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        }
                        _ => {}
                    }
                }
                Event::PointerButton { button: PointerButton::Extra1, pressed: true, .. } => self.go(-1),
                Event::PointerButton { button: PointerButton::Extra2, pressed: true, .. } => self.go(1),
                _ => {}
            }
        }
    }

    /// Wheel zooms around the cursor in fixed steps; pinch zooms freely.
    pub(super) fn handle_zoom_input(&mut self, ctx: &egui::Context, image: Vec2, canvas: Rect, ppp: f32) {
        let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        if !canvas.contains(pointer) {
            return;
        }
        let anchor = (pointer - canvas.center()) * ppp;
        let canvas_size = canvas.size() * ppp;
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                Event::MouseWheel { unit, delta, .. } => {
                    self.wheel += match unit {
                        MouseWheelUnit::Line | MouseWheelUnit::Page => delta.y,
                        MouseWheelUnit::Point => delta.y / POINTS_PER_STEP,
                    };
                    while self.wheel.abs() >= 0.999 {
                        let z = if self.wheel > 0.0 {
                            view::step_in(self.view.zoom)
                        } else {
                            view::step_out(self.view.zoom)
                        };
                        self.set_zoom(z, anchor, image, canvas_size);
                        self.wheel -= self.wheel.signum();
                    }
                }
                // Pinch is already continuous; follow the fingers exactly.
                Event::Zoom(factor) => {
                    self.view_from = None;
                    self.view.zoom_to(self.view.zoom * factor, anchor, image, canvas_size);
                }
                _ => {}
            }
        }
    }
}
