//! Windows Image Viewer: a fast, minimal image viewer for pixel art and photos.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod decode;
mod editor;
mod folder;
mod gallery;
mod icons;
mod loader;
mod platform;
mod register;
mod svg;
mod theme;
mod thumbs;
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
    if std::env::var_os("IMAGEVIEWER_TRACE_WINDOWS").is_some() {
        platform::trace_windows();
    }
    let mut options = app::Options {
        background: std::env::var_os("IMAGEVIEWER_BACKGROUND").is_some_and(|v| v == "1"),
        ..Default::default()
    };
    let mut size: Option<egui::Vec2> = None;
    let mut maximized = false;
    let mut dx12 = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--register") | Some("--unregister") => {
                let exe = std::env::current_exe().expect("own path");
                let result = if arg == "--register" { register::register(&exe) } else { register::unregister(&exe) };
                std::process::exit(if result.is_ok() { 0 } else { 1 });
            }
            Some("--maximized") => maximized = true,
            // Experiment: DirectX 12 through wgpu instead of OpenGL.
            Some("--dx12") => dx12 = true,
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

    if dx12 {
        // SAFETY: no other threads exist yet.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }

    // Start decoding the image straight away, in parallel with window and GPU start-up.
    if let Some(path) = options.open.as_ref().and_then(|p| std::path::absolute(p).ok()).filter(|p| p.is_file()) {
        let p = path.clone();
        let handle = std::thread::spawn(move || decode::decode(&p, 8192));
        options.preload = Some((path, handle));
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(register::APP_NAME)
        .with_app_id("WindowsImageViewer")
        .with_icon(icon)
        .with_inner_size(size.unwrap_or(egui::vec2(1100.0, 760.0)))
        .with_min_inner_size([720.0, 400.0])
        .with_drag_and_drop(true)
        .with_maximized(maximized);
    if options.background {
        // Tests and captures: off-screen and unfocused so nobody is interrupted.
        viewport = viewport.with_position([-12000.0, 0.0]).with_active(false).with_taskbar(false);
    }
    // winit maximizes (or fullscreens) a restored window straight after creating it, which
    // shows it before anything is drawn: a white flash. Create it normal-sized instead, and
    // let the app re-apply the state while the window is still cloaked.
    let restore = options.restore.clone();
    let window_builder: eframe::WindowBuilderHook = Box::new(move |mut builder| {
        let mut r = restore.lock().unwrap();
        r.maximized = builder.maximized == Some(true);
        r.fullscreen = builder.fullscreen == Some(true);
        builder.maximized = Some(false);
        builder.fullscreen = Some(false);
        builder
    });
    let native = eframe::NativeOptions {
        window_builder: Some(window_builder),
        viewport,
        renderer: if dx12 { eframe::Renderer::Wgpu } else { eframe::Renderer::Glow },
        persist_window: !options.background && size.is_none(),
        // Test runs keep their settings away from the user's.
        persistence_path: (options.background || options.capture.is_some()).then(|| std::env::temp_dir().join("imageviewer-test").join("app.ron")),
        // The saved position wins; first runs get Windows' default placement.
        centered: false,
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
