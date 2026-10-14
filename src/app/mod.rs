//! The viewer window: state, files and navigation, actions, and the frame loop.

mod canvas;
mod chrome;
mod dialogs;
mod harness;
mod input;
mod playback;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, ColorImage, CornerRadius, CursorIcon, Event, FontId, Key, Layout, Mesh, MouseWheelUnit,
    PointerButton, Pos2, Rect, RichText, Sense, Shape, Stroke, TextureHandle, TextureOptions, TextureWrapMode, Vec2,
    ViewportCommand, pos2, vec2,
};

use crate::editor::{self, Editor};
use crate::gallery::Gallery;
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
/// How often to check whether Windows switched between light and dark.
const THEME_CHECK: Duration = Duration::from_secs(1);
const CHECKER_TEXTURE: TextureOptions =
    TextureOptions { wrap_mode: TextureWrapMode::Repeat, ..TextureOptions::NEAREST };
/// How long a zoom change takes to ease in.
const VIEW_ANIMATION: Duration = Duration::from_millis(140);
/// While the next image decodes the previous one stays up; after this, say it's loading.
const LOADING_GRACE: Duration = Duration::from_millis(300);

/// What was last drawn, kept on screen while the next image decodes.
struct Shown {
    loaded: Arc<Loaded>,
    view: View,
    rotation: u8,
}

#[derive(Default)]
pub struct Options {
    pub open: Option<PathBuf>,
    /// Test harness: run these actions, save a screenshot here, then exit.
    pub script: Vec<String>,
    pub capture: Option<PathBuf>,
    pub background: bool,
    /// The startup image, already decoding while the window and GPU start.
    pub preload: Option<Preload>,
    /// Window state to re-apply once the window is cloaked (see main).
    pub restore: Arc<std::sync::Mutex<Restore>>,
}

#[derive(Default, Clone, Copy)]
pub struct Restore {
    pub maximized: bool,
    pub fullscreen: bool,
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
    /// Where the view is heading; `shown_view` eases towards it.
    view: View,
    view_from: Option<(View, Instant)>,
    view_for: Option<PathBuf>,
    current_since: Instant,
    last_shown: Option<Shown>,
    /// Bumped whenever `files` changes, to keep the cached index honest.
    files_generation: u64,
    fonts_checked: u64,
    index_cache: std::cell::RefCell<Option<(u64, PathBuf, Option<usize>)>>,
    shortcuts_open: bool,
    app_icon: TextureHandle,
    /// Quarter turns clockwise, per file, for this session only. Files are never rewritten.
    rotation: HashMap<PathBuf, u8>,
    anim: Animation,
    looping: bool,
    theme_choice: theme::Choice,
    /// The palette in use; follows Windows when the choice is `System`.
    theme_light: bool,
    last_theme_check: Instant,
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
    restore: Restore,
    last_inner: Option<Rect>,
    editor: Editor,
    /// Editors found on this machine, looked up when Settings first opens.
    editors: Option<Vec<Editor>>,
    settings_open: bool,
    gallery: Gallery,
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
        let stored = |key: &str| cc.storage.and_then(|s| s.get_string(key));
        // The theme is settled before anything is drawn (the window is still cloaked).
        theme::init(&cc.egui_ctx);
        let theme_choice = stored("theme").map_or(theme::Choice::System, |s| theme::Choice::from_storage(&s));
        let light = theme_choice.resolve();
        theme::apply(&cc.egui_ctx, light);
        let checker = cc.egui_ctx.load_texture("checker", checker_image(), CHECKER_TEXTURE);
        let mut app = Self {
            loader: Loader::new(cc.egui_ctx.clone()),
            files: Vec::new(),
            current: None,
            direction: 1,
            view: View::default(),
            view_from: None,
            view_for: None,
            current_since: Instant::now(),
            last_shown: None,
            files_generation: 0,
            fonts_checked: u64::MAX,
            index_cache: Default::default(),
            shortcuts_open: false,
            app_icon: cc.egui_ctx.load_texture("app-icon", app_icon_image(), TextureOptions::NEAREST),
            rotation: HashMap::new(),
            anim: Animation::default(),
            looping: stored("looping").is_none_or(|v| v == "true"),
            theme_choice,
            theme_light: light,
            last_theme_check: Instant::now(),
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
            restore: *options.restore.lock().unwrap(),
            last_inner: None,
            editor: stored("editor")
                .and_then(|s| Editor::from_storage(&s))
                .unwrap_or_else(|| editor::detect().into_iter().next().unwrap_or_else(Editor::windows_default)),
            editors: None,
            settings_open: false,
            gallery: Gallery::new(
                cc.egui_ctx.clone(),
                stored("gallery_open").is_some_and(|v| v == "true"),
                stored("gallery_card").and_then(|v| v.parse().ok()).unwrap_or(120.0),
            ),
            hover_pixel: None,
            script: options.script.into_iter().rev().collect(),
            capture: options.capture,
            frames_since_action: 0,
            started: Instant::now(),
            frames_traced: 0,
            shot_requested: false,
        };
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(cc)
            && let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw()
        {
            let hwnd = h.hwnd.get();
            app.hwnd = Some(hwnd);
            platform::set_cloaked(hwnd as _, true);
            platform::style_title_bar(hwnd as _, theme::pal().caption, theme::pal().caption_text, !light);
        }
        if app.restore.fullscreen {
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        } else if app.restore.maximized {
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
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

    /// Apply the chosen theme, and notice when Windows switches while we follow it.
    fn sync_theme(&mut self, ctx: &egui::Context, force: bool) {
        if !force && (self.theme_choice != theme::Choice::System || self.last_theme_check.elapsed() < THEME_CHECK) {
            return;
        }
        self.last_theme_check = Instant::now();
        let light = self.theme_choice.resolve();
        if light == self.theme_light && !force {
            return;
        }
        self.theme_light = light;
        theme::apply(ctx, light);
        self.checker.set(checker_image(), CHECKER_TEXTURE);
        if let Some(hwnd) = self.hwnd {
            let p = theme::pal();
            platform::style_title_bar(hwnd as _, p.caption, p.caption_text, !light);
        }
    }

    pub(super) fn set_theme(&mut self, ctx: &egui::Context, choice: theme::Choice) {
        self.theme_choice = choice;
        self.sync_theme(ctx, true);
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
        // Count frames with content at a settled size (a restored maximize resizes once).
        let (inner, maximized, fullscreen) = ctx.input(|i| {
            let v = i.viewport();
            (v.inner_rect, v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false))
        });
        let state_applied =
            (!self.restore.maximized || maximized || fullscreen) && (!self.restore.fullscreen || fullscreen);
        let settled = inner.is_some() && inner == self.last_inner;
        self.last_inner = inner;
        if ready && settled && state_applied {
            self.frames_ready += 1;
        } else {
            self.frames_ready = 0;
        }
        // One presented frame at the final size is enough.
        if self.frames_ready >= 1 || self.started.elapsed() > Duration::from_millis(1500) {
            self.revealed = true;
            if let Some(hwnd) = self.hwnd {
                platform::set_cloaked(hwnd as _, false);
            }
            crate::trace("revealed");
        } else {
            ctx.request_repaint();
        }
    }

    fn open(&mut self, path: &Path) {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if path.is_dir() {
            self.files = folder::scan(&path);
            self.files_generation += 1;
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
            self.files_generation += 1;
        }
        self.current = Some(path);
    }

    fn rescan_folder_of(&mut self, path: &Path) {
        if let Some(dir) = path.parent() {
            self.files = folder::scan(dir);
            self.files_generation += 1;
        }
    }

    /// The current file's place in the folder. Asked many times a frame, so cached.
    fn index(&self) -> Option<usize> {
        let current = self.current.as_deref()?;
        if let Some((generation, path, index)) = &*self.index_cache.borrow()
            && *generation == self.files_generation
            && path == current
        {
            return *index;
        }
        let index = folder::position(&self.files, current);
        *self.index_cache.borrow_mut() = Some((self.files_generation, current.to_path_buf(), index));
        index
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
            self.gallery.invalidate(&path);
        }
    }

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
        self.fit_view();
    }

    fn edit(&mut self) {
        let Some(path) = self.current.clone() else { return };
        if let Err(e) = self.editor.open(&path) {
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
                self.files_generation += 1;
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

    /// Zoom with a short ease (wheel, keys, buttons).
    fn set_zoom(&mut self, zoom: f32, anchor: Vec2, image: Vec2, canvas: Vec2) {
        self.begin_view_change();
        self.view.zoom_to(zoom, anchor, image, canvas);
    }

    fn fit_view(&mut self) {
        self.begin_view_change();
        self.view.fit = true;
    }

    /// Start easing from whatever is on screen now to the view about to be set.
    fn begin_view_change(&mut self) {
        self.view_from = Some((self.shown_view(), Instant::now()));
    }

    /// The view as drawn this frame.
    fn shown_view(&self) -> View {
        match self.view_from {
            Some((from, started)) => {
                let t = started.elapsed().as_secs_f32() / VIEW_ANIMATION.as_secs_f32();
                if t >= 1.0 { self.view } else { view::blend(from, self.view, view::ease_out(t)) }
            }
            None => self.view,
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        self.frame_ui(ui, frame);
        let spent = frame_start.elapsed();
        if crate::tracing() && spent > Duration::from_millis(4) {
            crate::trace(&format!("slow frame: {:.1} ms of UI work", spent.as_secs_f64() * 1000.0));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_settings(storage);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::pal().window.to_normalized_gamma_f32()
    }
}

impl ViewerApp {
    fn frame_ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        if self.frames_traced < 4 || self.capture.is_some() && self.frames_traced.is_multiple_of(30) {
            crate::trace(&format!(
                "frame {} ppp {ppp} script {:?} loaded {}",
                self.frames_traced,
                self.script,
                self.loaded().is_some()
            ));
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
        self.sync_theme(&ctx, false);

        // Files dropped on the window.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
            self.open(&path);
        }

        // Coming back to the window: the folder may have changed while we were away.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused
            && !self.was_focused
            && let Some(p) = self.current.clone()
        {
            self.rescan_folder_of(&p);
            if folder::position(&self.files, &p).is_none() && p.exists() {
                self.files.push(p);
                self.files_generation += 1;
            }
        }
        self.was_focused = focused;

        // Names in scripts Segoe UI lacks pull in Windows' CJK/Indic fonts, once.
        if self.fonts_checked != self.files_generation {
            self.fonts_checked = self.files_generation;
            let names: String = self.files.iter().map(|p| folder::file_name(p)).collect();
            let folder_name = self.current.as_deref().and_then(Path::parent).map(|p| p.to_string_lossy().into_owned());
            theme::ensure_fonts_for(&ctx, &(names + &folder_name.unwrap_or_default()));
        }
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
            self.view_from = None;
            self.view_for = self.current.clone();
            self.current_since = Instant::now();
            self.wheel = 0.0;
        }
        self.sync_animation(&ctx);
        self.update_title(&ctx);

        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        self.gallery.poll();
        if self.gallery.open && !fullscreen {
            let frame = egui::Frame::NONE.fill(theme::pal().window);
            let panel = egui::Panel::left("gallery")
                .resizable(true)
                .default_size(300.0)
                .size_range(160.0..=900.0)
                .frame(frame)
                .show(ui, |ui| self.gallery.ui(ui, &self.files, self.current.as_deref(), self.checker.id()));
            if let Some(path) = panel.inner
                && let Some(i) = folder::position(&self.files, &path)
            {
                self.go_to(i);
            }
        }
        let screen = ui.available_rect_before_wrap();
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

        let image = self.loaded().map(|l| canvas::display_size(&l, self.rotation()));
        if !ctx.egui_wants_keyboard_input() && !self.settings_open && !self.shortcuts_open {
            self.handle_keys(&ctx, image, canvas, ppp);
        }
        if let Some(img) = image {
            self.handle_zoom_input(&ctx, img, canvas, ppp);
        }

        ui.painter().rect_filled(screen, 0.0, theme::pal().window);
        self.canvas(ui, canvas, fullscreen, ppp);

        if show_info {
            let rect = Rect::from_min_size(screen.min, vec2(screen.width(), info_h));
            if fullscreen {
                ui.painter().rect_filled(rect, 0.0, theme::pal().window.gamma_multiply(0.92));
            }
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
            self.info_bar(&mut child);
        }
        if show_toolbar {
            let rect = Rect::from_min_max(pos2(screen.left(), screen.bottom() - bar_h), screen.max);
            if fullscreen {
                ui.painter().rect_filled(rect, 0.0, theme::pal().window.gamma_multiply(0.92));
            }
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
            self.toolbar(&mut child, &ctx, image, canvas, ppp);
        }
        self.settings_ui(&ctx);
        self.shortcuts_ui(&ctx);
        self.draw_toast(&ctx, canvas);
        self.drop_overlay(&ctx, screen);
        self.reveal_when_ready(&ctx);
        self.receive_capture(&ctx);
        self.run_script(&ctx, image, canvas, ppp);
    }

    fn save_settings(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("looping", self.looping.to_string());
        storage.set_string("theme", self.theme_choice.to_storage().into());
        storage.set_string("show_info", self.show_info.to_string());
        storage.set_string("editor", self.editor.to_storage());
        storage.set_string("gallery_open", self.gallery.open.to_string());
        storage.set_string("gallery_card", self.gallery.card.to_string());
    }
}

/// A 2×2 checkerboard in the active palette, tiled behind transparent images.
fn checker_image() -> ColorImage {
    let [a, b] = theme::pal().checker;
    ColorImage::new([2, 2], vec![a, b, b, a])
}

/// The app's own icon (for the empty window), from the same pixel art as the exe icon.
fn app_icon_image() -> ColorImage {
    let img = image::load_from_memory(include_bytes!("../../assets/icon.png")).map(|i| i.into_rgba8());
    match img {
        Ok(img) => ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img),
        Err(_) => ColorImage::new([1, 1], vec![Color32::TRANSPARENT]),
    }
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
