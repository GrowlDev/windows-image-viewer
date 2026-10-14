//! Windows Image Viewer: a fast, minimal image viewer for pixel art and photos.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod decode;
mod folder;
mod icons;
mod loader;
mod platform;
mod register;
mod theme;
mod view;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use eframe::egui;

static START: OnceLock<Instant> = OnceLock::new();

/// Developer trace: set IMAGEVIEWER_TRACE=<file> to log startup and test-harness milestones.
pub fn trace(msg: &str) {
    use std::io::Write;
    let Some(path) = std::env::var_os("IMAGEVIEWER_TRACE") else { return };
    let ms = START.get().map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{ms:8.1} ms  {msg}");
    }
}

fn main() -> eframe::Result {
    START.get_or_init(Instant::now);
    let mut options = app::Options {
        background: std::env::var_os("IMAGEVIEWER_BACKGROUND").is_some_and(|v| v == "1"),
        ..Default::default()
    };
    let mut size: Option<egui::Vec2> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--register") | Some("--unregister") => {
                let exe = std::env::current_exe().expect("own path");
                let result = if arg == "--register" { register::register(&exe) } else { register::unregister(&exe) };
                std::process::exit(if result.is_ok() { 0 } else { 1 });
            }
            Some("--capture") => options.capture = args.next().map(PathBuf::from),
            Some("--do") => {
                let list = args.next().unwrap_or_default();
                options.script = list.to_string_lossy().split(',').map(str::to_owned).collect();
            }
            Some("--size") => {
                let s = args.next().unwrap_or_default().to_string_lossy().into_owned();
                if let Some((w, h)) = s.split_once('x') {
                    size = Some(egui::vec2(w.parse().unwrap_or(1100.0), h.parse().unwrap_or(760.0)));
                }
            }
            _ => options.open = Some(PathBuf::from(arg)),
        }
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(register::APP_NAME)
        .with_app_id("WindowsImageViewer")
        .with_icon(icon)
        .with_inner_size(size.unwrap_or(egui::vec2(1100.0, 760.0)))
        .with_min_inner_size([420.0, 260.0])
        .with_drag_and_drop(true);
    if options.background {
        // Tests and captures: off-screen and unfocused so nobody is interrupted.
        viewport = viewport.with_position([-12000.0, 0.0]).with_active(false).with_taskbar(false);
    }
    let native = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        persist_window: !options.background && size.is_none(),
        centered: true,
        ..Default::default()
    };
    trace("run_native");
    eframe::run_native(
        register::APP_NAME,
        native,
        Box::new(|cc| {
            trace("creation context ready");
            let app = app::ViewerApp::new(cc, options);
            trace("app ready");
            Ok(Box::new(app))
        }),
    )
}
