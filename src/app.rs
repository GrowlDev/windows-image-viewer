//! The viewer window: info bar, canvas, toolbar, input and animation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, ColorImage, CornerRadius, CursorIcon, Event, FontId, Key, Layout,
    Mesh, MouseWheelUnit, PointerButton, Pos2, Rect, RichText, Sense, Shape, Stroke,
    TextureHandle, TextureOptions, TextureWrapMode, Vec2, ViewportCommand, pos2, vec2,
};

use crate::icons::{self, Icon};
use crate::loader::{IMAGE_TEXTURE, Loaded, Loader, Slot};
use crate::view::{self, View};
use crate::{decode, folder, platform, register, theme};

/// Canvas breathing room around a fitted image, in points.
const PADDING: f32 = 16.0;
/// How often the shown file is checked for changes (an editor saving over it).
const FILE_CHECK: Duration = Duration::from_millis(700);
/// Wheel distance in points that counts as one zoom step on smooth-scrolling devices.
const POINTS_PER_STEP: f32 = 50.0;

#[derive(Default)]
pub struct Options {
    pub open: Option<PathBuf>,
    /// Test harness: run these actions, save a screenshot here, then exit.
    pub script: Vec<String>,
    pub capture: Option<PathBuf>,
    pub background: bool,
    /// The startup image, already decoding while the window and GPU start.
    pub preload: Option<Preload>,
}

pub type Preload = (PathBuf, std::thread::JoinHandle<Result<decode::Decoded, String>>);

#[derive(Default)]
struct Animation {
    path: Option<PathBuf>,
    frame: usize,
    playing: bool,
    due: Option<Instant>,
}

pub struct ViewerApp {
    loader: Loader,
    files: Vec<PathBuf>,
    current: Option<PathBuf>,
    /// +1 or -1: which way the user is browsing, so we preload ahead of them.
    direction: isize,
    view: View,
    view_for: Option<PathBuf>,
    /// Quarter turns clockwise, per file, for this session only. Files are never rewritten.
    rotation: HashMap<PathBuf, u8>,
    anim: Animation,
    looping: bool,
    show_info: bool,
    checker: TextureHandle,
    toast: Option<(String, bool, Instant)>,
    wheel: f32,
    last_file_check: Instant,
    was_focused: bool,
    title: String,
    first_frame_done: bool,
    hwnd: Option<isize>,
    preload: Option<Preload>,
    /// The window stays cloaked until a frame with the image in it is on screen.
    revealed: bool,
    frames_ready: u32,
    hover_pixel: Option<([u32; 2], Option<Color32>)>,
    script: Vec<String>,
    capture: Option<PathBuf>,
    frames_since_action: u32,
    started: Instant,
    frames_traced: u64,
    shot_requested: bool,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        theme::apply(&cc.egui_ctx);
        let checker = cc.egui_ctx.load_texture(
            "checker",
            ColorImage::new([2, 2], vec![theme::CHECKER[0], theme::CHECKER[1], theme::CHECKER[1], theme::CHECKER[0]]),
            TextureOptions { wrap_mode: TextureWrapMode::Repeat, ..TextureOptions::NEAREST },
        );
        let stored = |key: &str| cc.storage.and_then(|s| s.get_string(key));
        let mut app = Self {
            loader: Loader::new(cc.egui_ctx.clone()),
            files: Vec::new(),
            current: None,
            direction: 1,
            view: View::default(),
            view_for: None,
            rotation: HashMap::new(),
            anim: Animation::default(),
            looping: stored("looping").is_none_or(|v| v == "true"),
            show_info: stored("show_info").is_none_or(|v| v == "true"),
            checker,
            toast: None,
            wheel: 0.0,
            last_file_check: Instant::now(),
            was_focused: true,
            title: String::new(),
            first_frame_done: false,
            hwnd: None,
            preload: None,
            revealed: false,
            frames_ready: 0,
            hover_pixel: None,
            script: options.script.into_iter().rev().collect(),
            capture: options.capture,
            frames_since_action: 0,
            started: Instant::now(),
            frames_traced: 0,
            shot_requested: false,
        };
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(cc) {
            if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                let hwnd = h.hwnd.get();
                app.hwnd = Some(hwnd);
                platform::set_cloaked(hwnd as _, true);
                platform::style_title_bar(hwnd as _, theme::CAPTION, theme::CAPTION_TEXT);
            }
        }
        app.preload = options.preload;
        // Usually the decode beat the GPU start; give a slow one a moment before going async.
        let deadline = Instant::now() + Duration::from_millis(250);
        while app.preload.as_ref().is_some_and(|(_, h)| !h.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        app.take_preload();
        if let Some(path) = options.open {
            app.open(&path);
        }
        app
    }

    fn take_preload(&mut self) {
        if self.preload.as_ref().is_some_and(|(_, h)| h.is_finished()) {
            let (path, handle) = self.preload.take().unwrap();
            let result = handle.join().unwrap_or_else(|_| Err("Decoder crashed".into()));
            self.loader.insert(path, result);
        }
    }

    /// Show the window once a frame with real content has been presented.
    fn reveal_when_ready(&mut self, ctx: &egui::Context) {
        if self.revealed {
            return;
        }
        let ready = match self.current.as_deref() {
            None => true,
            Some(p) => matches!(self.loader.get(p), Some(Slot::Ready(_) | Slot::Failed(_))),
        };
        if ready {
            self.frames_ready += 1;
        }
        if self.frames_ready >= 2 || self.started.elapsed() > Duration::from_millis(1500) {
            self.revealed = true;
            if let Some(hwnd) = self.hwnd {
                platform::set_cloaked(hwnd as _, false);
            }
            crate::trace("revealed");
        } else {
            ctx.request_repaint();
        }
    }

    // ---------------------------------------------------------------- files

    fn open(&mut self, path: &Path) {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if path.is_dir() {
            self.files = folder::scan(&path);
            self.current = self.files.first().cloned();
            if self.current.is_none() {
                self.notify("No images in that folder", true);
            }
            return;
        }
        self.rescan_folder_of(&path);
        if folder::position(&self.files, &path).is_none() {
            // Opened explicitly with an unusual extension: still show it, in name order.
            let at = self
                .files
                .partition_point(|p| folder::explorer_cmp(folder::file_name(p), folder::file_name(&path)).is_lt());
            self.files.insert(at, path.clone());
        }
        self.current = Some(path);
    }

    fn rescan_folder_of(&mut self, path: &Path) {
        if let Some(dir) = path.parent() {
            self.files = folder::scan(dir);
        }
    }

    fn index(&self) -> Option<usize> {
        self.current.as_deref().and_then(|c| folder::position(&self.files, c))
    }

    fn go(&mut self, step: isize) {
        let (Some(i), n) = (self.index(), self.files.len() as isize) else { return };
        if n == 0 {
            return;
        }
        self.direction = step.signum();
        let next = (i as isize + step).rem_euclid(n) as usize;
        self.current = Some(self.files[next].clone());
    }

    fn go_to(&mut self, index: usize) {
        if let Some(path) = self.files.get(index) {
            self.direction = if Some(index) < self.index() { -1 } else { 1 };
            self.current = Some(path.clone());
        }
    }

    /// The current image and its neighbours, most urgent first.
    fn wanted(&self) -> Vec<PathBuf> {
        let Some(current) = self.current.clone() else { return Vec::new() };
        let mut out = vec![current];
        if let Some(i) = self.index() {
            let n = self.files.len() as isize;
            let d = self.direction;
            for off in [d, -d, 2 * d] {
                let p = &self.files[(i as isize + off).rem_euclid(n) as usize];
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
        }
        out
    }

    fn loaded(&self) -> Option<Arc<Loaded>> {
        match self.loader.get(self.current.as_deref()?) {
            Some(Slot::Ready(l)) => Some(l.clone()),
            _ => None,
        }
    }

    /// Pick up edits saved by Paint.NET (or anything else) while we're open.
    fn check_current_file(&mut self) {
        if self.last_file_check.elapsed() < FILE_CHECK {
            return;
        }
        self.last_file_check = Instant::now();
        let (Some(path), Some(loaded)) = (self.current.clone(), self.loaded()) else { return };
        let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) else { return };
        if loaded.info.modified != Some(modified) {
            self.loader.reload(&path);
        }
    }

    // -------------------------------------------------------------- actions

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some((text.into(), error, Instant::now()));
    }

    fn rotation(&self) -> u8 {
        self.current.as_ref().and_then(|p| self.rotation.get(p).copied()).unwrap_or(0)
    }

    fn rotate(&mut self, quarter_turns: u8) {
        let Some(path) = self.current.clone() else { return };
        let r = self.rotation.entry(path).or_default();
        *r = (*r + quarter_turns) % 4;
        self.view.fit = true;
    }

    fn edit(&mut self) {
        let Some(path) = self.current.clone() else { return };
        if let Err(e) = platform::edit_in_paint_net(&path) {
            self.notify(e, true);
        }
    }

    fn delete(&mut self) {
        let Some(path) = self.current.clone() else { return };
        let index = self.index();
        match platform::recycle(&path) {
            Ok(()) => {
                self.loader.forget(&path);
                self.rotation.remove(&path);
                self.files.retain(|p| p != &path);
                self.current = index.and_then(|i| self.files.get(i.min(self.files.len().saturating_sub(1)))).cloned();
                self.notify(format!("Moved {} to the Recycle Bin", folder::file_name(&path)), false);
            }
            Err(e) => self.notify(e, true),
        }
    }

    fn copy(&mut self) {
        let Some(path) = self.current.clone() else { return };
        let frame = self.loaded().and_then(|l| {
            let f = l.info.frames.get(self.anim.frame.min(l.info.frames.len().saturating_sub(1)))?;
            let full = f.image.size == [l.info.width as usize, l.info.height as usize];
            full.then(|| unmultiply(&f.image))
        });
        let result = frame
            .map(Ok)
            .unwrap_or_else(|| decode::full_rgba(&path))
            .map(|img| rotate_rgba(img, self.rotation()))
            .and_then(|img| {
                let (w, h) = img.dimensions();
                arboard::Clipboard::new()
                    .and_then(|mut c| {
                        c.set_image(arboard::ImageData {
                            width: w as usize,
                            height: h as usize,
                            bytes: img.into_raw().into(),
                        })
                    })
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => self.notify("Copied to the clipboard", false),
            Err(e) => self.notify(format!("Couldn't copy: {e}"), true),
        }
    }

    fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("Images", folder::EXTENSIONS);
        if let Some(dir) = self.current.as_deref().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open(&path);
        }
    }

    fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!full));
    }

    fn set_zoom(&mut self, zoom: f32, anchor: Vec2, image: Vec2, canvas: Vec2) {
        self.view.zoom_to(zoom, anchor, image, canvas);
    }

    // ---------------------------------------------------------------- input

    fn handle_keys(&mut self, ctx: &egui::Context, image: Option<Vec2>, canvas: Rect, ppp: f32) {
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
                        Key::Num0 => self.view.fit = true,
                        Key::Num1 if !ctrl => zoom_to(self, 1.0),
                        Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8
                        | Key::Num9 if !ctrl => {
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
                        Key::E => self.edit(),
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
    fn handle_zoom_input(&mut self, ctx: &egui::Context, image: Vec2, canvas: Rect, ppp: f32) {
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
                        let z = if self.wheel > 0.0 { view::step_in(self.view.zoom) } else { view::step_out(self.view.zoom) };
                        self.set_zoom(z, anchor, image, canvas_size);
                        self.wheel -= self.wheel.signum();
                    }
                }
                Event::Zoom(factor) => {
                    self.set_zoom(self.view.zoom * factor, anchor, image, canvas_size);
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------ animation

    fn sync_animation(&mut self, ctx: &egui::Context) {
        let Some(loaded) = self.loaded().filter(|l| l.info.is_animated()) else {
            self.anim = Animation::default();
            return;
        };
        let frames = &loaded.info.frames;
        let mut texture = loaded.texture.clone();
        if self.anim.path != self.current {
            self.anim = Animation { path: self.current.clone(), frame: 0, playing: true, due: None };
            texture.set(frames[0].image.clone(), IMAGE_TEXTURE);
        }
        self.anim.frame = self.anim.frame.min(frames.len() - 1);
        if !self.anim.playing {
            self.anim.due = None;
            return;
        }
        let now = Instant::now();
        let due = *self.anim.due.get_or_insert(now + frames[self.anim.frame].delay);
        if now >= due {
            let mut frame = self.anim.frame;
            let mut next_due = due;
            while now >= next_due {
                if frame + 1 == frames.len() {
                    if !self.looping {
                        self.anim.playing = false;
                        break;
                    }
                    frame = 0;
                } else {
                    frame += 1;
                }
                next_due += frames[frame].delay;
            }
            // After a stall (window hidden, slow decode) restart the clock instead of racing.
            if now.duration_since(due) > Duration::from_secs(1) {
                next_due = now + frames[frame].delay;
            }
            if frame != self.anim.frame {
                self.anim.frame = frame;
                texture.set(frames[frame].image.clone(), IMAGE_TEXTURE);
            }
            self.anim.due = self.anim.playing.then_some(next_due);
        }
        if let Some(due) = self.anim.due {
            ctx.request_repaint_after(due.saturating_duration_since(now));
        }
    }

    fn toggle_play(&mut self) {
        let Some(l) = self.loaded().filter(|l| l.info.is_animated()) else { return };
        if !self.anim.playing && self.anim.frame + 1 == l.info.frames.len() && !self.looping {
            self.show_frame(0);
        }
        self.anim.playing = !self.anim.playing;
        self.anim.due = None;
    }

    fn toggle_loop(&mut self) {
        self.looping = !self.looping;
    }

    fn step_frame(&mut self, step: isize) {
        let Some(l) = self.loaded().filter(|l| l.info.is_animated()) else { return };
        let n = l.info.frames.len() as isize;
        self.anim.playing = false;
        self.show_frame((self.anim.frame as isize + step).rem_euclid(n) as usize);
    }

    fn show_frame(&mut self, frame: usize) {
        let Some(l) = self.loaded() else { return };
        if let Some(f) = l.info.frames.get(frame) {
            self.anim.frame = frame;
            self.anim.due = None;
            l.texture.clone().set(f.image.clone(), IMAGE_TEXTURE);
        }
    }

    // ------------------------------------------------------------------- ui

    fn info_bar(&mut self, ui: &mut egui::Ui) {
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
                    let mut parts = vec![format!("{} × {}", i.width, i.height), i.format.to_string(), platform::file_size(i.file_size)];
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

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, image: Option<Vec2>, canvas: Rect, ppp: f32) {
        let has_image = image.is_some();
        let has_file = self.current.is_some();
        let many = self.files.len() > 1;
        let canvas_size = canvas.size() * ppp;
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            if icons::button(ui, Icon::Prev, "Previous  (←)", many, false) {
                self.go(-1);
            }
            if icons::button(ui, Icon::Next, "Next  (→)", many, false) {
                self.go(1);
            }
            divider(ui);
            if icons::button(ui, Icon::ZoomOut, "Zoom out  (−, wheel)", has_image, false) {
                if let Some(img) = image {
                    self.set_zoom(view::step_out(self.view.zoom), Vec2::ZERO, img, canvas_size);
                }
            }
            let label = RichText::new(view::percent(self.view.zoom)).font(FontId::proportional(14.0));
            let pct = ui.add_enabled(has_image, egui::Button::new(label).min_size(vec2(58.0, 30.0)).frame(false));
            if pct.on_hover_text("100%  (1)  ·  click to toggle Fit  (0)").clicked() {
                if self.view.zoom == 1.0 && !self.view.fit {
                    self.view.fit = true;
                } else if let Some(img) = image {
                    self.set_zoom(1.0, Vec2::ZERO, img, canvas_size);
                }
            }
            if icons::button(ui, Icon::ZoomIn, "Zoom in  (+, wheel)", has_image, false) {
                if let Some(img) = image {
                    self.set_zoom(view::step_in(self.view.zoom), Vec2::ZERO, img, canvas_size);
                }
            }
            if icons::button(ui, Icon::Fit, "Fit to window  (0)", has_image, self.view.fit) {
                self.view.fit = true;
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
                let (icon, tip) = if self.anim.playing { (Icon::Pause, "Pause  (Space)") } else { (Icon::Play, "Play  (Space)") };
                if icons::button(ui, icon, tip, true, false) {
                    self.toggle_play();
                }
                let n = l.info.frames.len();
                let frame_label = RichText::new(format!("{:>w$} / {n}", self.anim.frame + 1, w = n.to_string().len()))
                    .color(theme::TEXT_DIM)
                    .monospace();
                ui.label(frame_label).on_hover_text("Step frames with , and .");
                if icons::button(ui, Icon::Loop, if self.looping { "Loop is on  (L)" } else { "Loop is off  (L)" }, true, self.looping) {
                    self.toggle_loop();
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if icons::text_button(ui, Icon::Edit, "Edit in Paint.NET", "Open in Paint.NET  (E)
Saved changes show here automatically.", has_file) {
                    self.edit();
                }
                ui.add_space(6.0);
                if icons::button(ui, Icon::Delete, "Move to Recycle Bin  (Del)", has_file, false) {
                    self.delete();
                }
                if icons::button(ui, Icon::OpenWith, "Open with…", has_file, false) {
                    if let (Some(hwnd), Some(p)) = (self.hwnd, self.current.clone()) {
                        platform::open_with(hwnd as _, &p);
                    }
                }
                if icons::button(ui, Icon::Folder, "Show in Explorer", has_file, false) {
                    if let Some(p) = self.current.clone() {
                        if let Err(e) = platform::show_in_explorer(&p) {
                            self.notify(e, true);
                        }
                    }
                }
                if icons::button(ui, Icon::Copy, "Copy image  (Ctrl+C)", has_image, false) {
                    self.copy();
                }
                if icons::button(ui, Icon::Info, "File info bar  (I)", true, self.show_info) {
                    self.show_info = !self.show_info;
                }
                divider(ui);
                self.pixel_readout(ui);
            });
        });
        let _ = ctx;
    }

    /// Coordinates and colour of the pixel under the cursor.
    fn pixel_readout(&self, ui: &mut egui::Ui) {
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

    fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, fullscreen: bool, ppp: f32) {
        let painter = ui.painter_at(rect);
        if fullscreen {
            painter.rect_filled(rect, 0.0, theme::FULLSCREEN);
        } else {
            painter.rect_filled(rect, CornerRadius::same(8), theme::CANVAS);
        }
        let response = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
        if response.double_clicked() {
            self.toggle_fullscreen(ui.ctx());
        }
        self.hover_pixel = None;

        let Some(path) = self.current.clone() else {
            centered_text(&painter, rect, "Open an image  (Ctrl+O)  or drop one here", theme::TEXT_DIM);
            return;
        };
        let loaded = match self.loader.get(&path) {
            Some(Slot::Ready(l)) => l.clone(),
            Some(Slot::Failed(e)) => {
                centered_text(&painter, rect, &format!("Can't show {}\n{e}", folder::file_name(&path)), theme::DANGER);
                return;
            }
            _ => {
                if self.started.elapsed() > Duration::from_millis(150) {
                    centered_text(&painter, rect, "Loading…", theme::TEXT_DIM);
                }
                return;
            }
        };

        let rotation = self.rotation();
        let image = display_size(&loaded, rotation);
        let canvas_px = Rect::from_min_max(
            pos2((rect.min.x * ppp).round(), (rect.min.y * ppp).round()),
            pos2((rect.max.x * ppp).round(), (rect.max.y * ppp).round()),
        );
        self.view.settle(image, canvas_px.size(), (PADDING * ppp).round());

        if response.dragged() {
            self.view.pan_by(response.drag_delta() * ppp, image, canvas_px.size());
        }
        let pannable = image.x * self.view.zoom > canvas_px.width() + 0.5 || image.y * self.view.zoom > canvas_px.height() + 0.5;
        if response.dragged() && pannable {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() && pannable {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }

        let shown_px = self.view.image_rect(canvas_px, image);
        let shown = Rect::from_min_max((shown_px.min.to_vec2() / ppp).to_pos2(), (shown_px.max.to_vec2() / ppp).to_pos2());

        if loaded.info.has_alpha {
            let cell = (8.0 * ppp).round().max(4.0);
            let uv = Rect::from_min_max(Pos2::ZERO, (shown_px.size() / (cell * 2.0)).to_pos2());
            painter.image(self.checker.id(), shown, uv, Color32::WHITE);
        }
        painter.add(Shape::mesh(rotated_quad(loaded.texture.id(), shown, rotation)));

        if let Some(pointer) = response.hover_pos() {
            let p = pos2(pointer.x * ppp, pointer.y * ppp);
            if let Some(px) = self.view.pixel_at(canvas_px, image, p) {
                let [w, h] = [loaded.info.width, loaded.info.height];
                let src = unrotate(px, rotation, w, h);
                self.hover_pixel = Some((src, pixel_color(&loaded, self.anim.frame, src)));
            }
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context, canvas: Rect) {
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
        painter.rect_filled(rect, CornerRadius::same(6), Color32::from_rgba_unmultiplied(16, 18, 21, (235.0 * alpha) as u8));
        painter.galley(rect.min + vec2(14.0, 8.0), galley, color);
        ctx.request_repaint();
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.current {
            Some(p) => format!("{} - {}", folder::file_name(p), register::APP_NAME),
            None => register::APP_NAME.to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Test harness: one scripted action per settled frame, then a screenshot.
    fn run_script(&mut self, ctx: &egui::Context, image: Option<Vec2>, canvas: Rect, ppp: f32) {
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
                "fit" => self.view.fit = true,
                "rotate" => self.rotate(1),
                "pause" => self.toggle_play(),
                "step" => self.step_frame(1),
                "info" => self.show_info = !self.show_info,
                "fullscreen" => self.toggle_fullscreen(ctx),
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
    fn receive_capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.capture.clone() else { return };
        for event in ctx.input(|i| i.events.clone()) {
            if let Event::Screenshot { image, .. } = event {
                crate::trace("screenshot received");
                let [w, h] = image.size;
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                    let _ = img.save(&path);
                }
                self.capture = None;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        if self.frames_traced < 4 || self.capture.is_some() && self.frames_traced % 30 == 0 {
            crate::trace(&format!("frame {} ppp {ppp} script {:?} loaded {}", self.frames_traced, self.script, self.loaded().is_some()));
        }
        self.frames_traced += 1;

        let _ = frame;
        if !self.first_frame_done {
            self.first_frame_done = true;
            let max_side = ctx.input(|i| i.max_texture_side);
            self.loader.set_max_texture_side(max_side);
            // The startup decode assumed a conservative texture limit; redo it if this GPU
            // could show more of a huge image.
            if let (Some(l), Some(p)) = (self.loaded(), self.current.clone()) {
                let [tw, th] = l.texture.size();
                if (tw as u32) < l.info.width.max(l.info.height) && tw.max(th) < max_side {
                    self.loader.reload(&p);
                }
            }
        }
        self.take_preload();

        // Files dropped on the window.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
            self.open(&path);
        }

        // Coming back to the window: the folder may have changed while we were away.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.was_focused {
            if let Some(p) = self.current.clone() {
                self.rescan_folder_of(&p);
                if folder::position(&self.files, &p).is_none() && p.exists() {
                    self.files.push(p);
                }
            }
        }
        self.was_focused = focused;

        self.loader.poll();
        self.check_current_file();
        let mut wanted = self.wanted();
        if let Some((pending, _)) = &self.preload {
            wanted.retain(|p| p != pending);
        }
        self.loader.want(&wanted);
        ctx.request_repaint_after(FILE_CHECK);

        // Reset the view when the image changes; keep it when the same file reloads.
        if self.view_for != self.current {
            self.view = View::default();
            self.view_for = self.current.clone();
            self.wheel = 0.0;
        }
        self.sync_animation(&ctx);
        self.update_title(&ctx);

        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let screen = ui.max_rect();
        let pointer_y = ctx.input(|i| i.pointer.hover_pos()).map(|p| p.y);
        let show_bars_full = |near_top: bool| {
            pointer_y.is_some_and(|y| if near_top { y < screen.top() + 60.0 } else { y > screen.bottom() - 70.0 })
        };
        let show_info = self.show_info && (!fullscreen || show_bars_full(true));
        let show_toolbar = !fullscreen || show_bars_full(false);

        let info_h = if show_info { 36.0 } else { 0.0 };
        let bar_h = 48.0;
        let canvas = if fullscreen {
            screen
        } else {
            let margin = 10.0;
            Rect::from_min_max(
                pos2(screen.left() + margin, screen.top() + if show_info { info_h } else { margin }),
                pos2(screen.right() - margin, screen.bottom() - bar_h),
            )
        };

        let image = self.loaded().map(|l| display_size(&l, self.rotation()));
        if !ctx.egui_wants_keyboard_input() {
            self.handle_keys(&ctx, image, canvas, ppp);
        }
        if let Some(img) = image {
            self.handle_zoom_input(&ctx, img, canvas, ppp);
        }

        ui.painter().rect_filled(screen, 0.0, theme::WINDOW);
        self.canvas(ui, canvas, fullscreen, ppp);

        if show_info {
            let rect = Rect::from_min_size(screen.min, vec2(screen.width(), info_h));
            if fullscreen {
                ui.painter().rect_filled(rect, 0.0, theme::WINDOW.gamma_multiply(0.92));
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
            self.info_bar(&mut child);
        }
        if show_toolbar {
            let rect = Rect::from_min_max(pos2(screen.left(), screen.bottom() - bar_h), screen.max);
            if fullscreen {
                ui.painter().rect_filled(rect, 0.0, theme::WINDOW.gamma_multiply(0.92));
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
            self.toolbar(&mut child, &ctx, image, canvas, ppp);
        }
        self.draw_toast(&ctx, canvas);
        self.reveal_when_ready(&ctx);
        self.receive_capture(&ctx);
        self.run_script(&ctx, image, canvas, ppp);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("looping", self.looping.to_string());
        storage.set_string("show_info", self.show_info.to_string());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::WINDOW.to_normalized_gamma_f32()
    }
}

// ------------------------------------------------------------------ helpers

fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::DIVIDER);
    ui.add_space(4.0);
}

fn centered_text(painter: &egui::Painter, rect: Rect, text: &str, color: Color32) {
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(15.0), color);
}

fn display_size(loaded: &Loaded, rotation: u8) -> Vec2 {
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

fn unmultiply(image: &ColorImage) -> image::RgbaImage {
    let [w, h] = image.size;
    let bytes = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    image::RgbaImage::from_raw(w as u32, h as u32, bytes).expect("size matches")
}

fn rotate_rgba(img: image::RgbaImage, rotation: u8) -> image::RgbaImage {
    match rotation % 4 {
        1 => image::imageops::rotate90(&img),
        2 => image::imageops::rotate180(&img),
        3 => image::imageops::rotate270(&img),
        _ => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrotate_round_trips_corners() {
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
    fn quad_rotation_matches_unrotate() {
        let rect = Rect::from_min_max(Pos2::ZERO, pos2(2.0, 4.0));
        let mesh = rotated_quad(egui::TextureId::default(), rect, 1);
        // Display top-left samples the texture's bottom-left.
        assert_eq!(mesh.vertices[0].uv, pos2(0.0, 1.0));
        // Display top-right samples the texture's top-left.
        assert_eq!(mesh.vertices[1].uv, pos2(0.0, 0.0));
    }
}
