//! Colours, fonts and egui style.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

/// Every colour the app draws with. Two palettes; `pal()` returns the active one.
pub struct Palette {
    /// Window frame, bars and dialogs.
    pub window: Color32,
    /// Behind the image. Neutral grey in both themes so it never tints how images look.
    pub canvas: Color32,
    pub fullscreen: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub button_hover: Color32,
    pub button_down: Color32,
    /// Checkbox, radio and slider faces.
    pub control: Color32,
    pub control_hover: Color32,
    /// A button that has to stand out from the canvas (the empty window's Open button).
    pub raised: Color32,
    pub divider: Color32,
    pub accent: Color32,
    pub danger: Color32,
    pub checker: [Color32; 2],
    /// Title bar background and text, matched to the window so the frame reads as one surface.
    pub caption: [u8; 3],
    pub caption_text: [u8; 3],
}

pub const DARK: Palette = Palette {
    window: Color32::from_rgb(27, 30, 35),
    canvas: Color32::from_rgb(38, 42, 49),
    fullscreen: Color32::from_rgb(12, 13, 15),
    text: Color32::from_rgb(220, 223, 228),
    text_dim: Color32::from_rgb(143, 149, 160),
    text_disabled: Color32::from_rgb(76, 81, 90),
    button_hover: Color32::from_rgb(45, 49, 57),
    button_down: Color32::from_rgb(55, 60, 69),
    control: Color32::from_rgb(52, 57, 66),
    control_hover: Color32::from_rgb(66, 72, 83),
    raised: Color32::from_rgb(52, 57, 66),
    divider: Color32::from_rgb(48, 52, 60),
    accent: Color32::from_rgb(190, 204, 160),
    danger: Color32::from_rgb(232, 128, 116),
    checker: [Color32::from_rgb(44, 48, 55), Color32::from_rgb(53, 57, 65)],
    caption: [27, 30, 35],
    caption_text: [200, 204, 211],
};

pub const LIGHT: Palette = Palette {
    window: Color32::from_rgb(243, 243, 245),
    canvas: Color32::from_rgb(226, 227, 230),
    fullscreen: Color32::from_rgb(236, 236, 238),
    text: Color32::from_rgb(28, 30, 34),
    text_dim: Color32::from_rgb(96, 100, 108),
    text_disabled: Color32::from_rgb(172, 175, 181),
    button_hover: Color32::from_rgb(225, 226, 230),
    button_down: Color32::from_rgb(211, 213, 218),
    control: Color32::from_rgb(214, 216, 221),
    control_hover: Color32::from_rgb(198, 201, 207),
    raised: Color32::from_rgb(252, 252, 253),
    divider: Color32::from_rgb(214, 216, 220),
    accent: Color32::from_rgb(78, 112, 66),
    danger: Color32::from_rgb(192, 62, 50),
    checker: [Color32::from_rgb(238, 238, 240), Color32::from_rgb(222, 223, 226)],
    caption: [243, 243, 245],
    caption_text: [28, 30, 34],
};

/// Toasts and the loading pill are a dark heads-up display in both themes.
pub const HUD: Color32 = Color32::from_rgb(16, 18, 21);
pub const HUD_TEXT: Color32 = Color32::from_rgb(230, 232, 236);
pub const HUD_ERROR: Color32 = Color32::from_rgb(240, 140, 128);

static LIGHT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The active palette.
pub fn pal() -> &'static Palette {
    if LIGHT_ACTIVE.load(Ordering::Relaxed) { &LIGHT } else { &DARK }
}

/// The user's choice in Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    System,
    Dark,
    Light,
}

impl Choice {
    pub const ALL: [Choice; 3] = [Choice::System, Choice::Dark, Choice::Light];

    pub fn label(self) -> &'static str {
        match self {
            Choice::System => "Match Windows",
            Choice::Dark => "Dark",
            Choice::Light => "Light",
        }
    }

    pub fn to_storage(self) -> &'static str {
        match self {
            Choice::System => "system",
            Choice::Dark => "dark",
            Choice::Light => "light",
        }
    }

    pub fn from_storage(s: &str) -> Self {
        match s {
            "dark" => Choice::Dark,
            "light" => Choice::Light,
            _ => Choice::System,
        }
    }

    /// Light or not, asking Windows when following it.
    pub fn resolve(self) -> bool {
        match self {
            Choice::System => crate::platform::windows_uses_light_theme(),
            Choice::Dark => false,
            Choice::Light => true,
        }
    }
}

/// Fonts and the parts of the style that don't depend on the palette. Once, at startup.
pub fn init(ctx: &egui::Context) {
    install_fonts(ctx);
}

/// Switch every egui style colour to the light or dark palette.
pub fn apply(ctx: &egui::Context, light: bool) {
    LIGHT_ACTIVE.store(light, Ordering::Relaxed);
    let p = pal();
    ctx.set_theme(if light { egui::ThemePreference::Light } else { egui::ThemePreference::Dark });
    ctx.all_styles_mut(|style| {
        style.visuals = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
        let v = &mut style.visuals;
        v.panel_fill = p.window;
        v.window_fill = p.window;
        v.override_text_color = Some(p.text);
        v.widgets.noninteractive.bg_stroke.color = p.divider;
        // Buttons: frameless until hovered. Checkboxes, radios and slider tracks: visible.
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = p.control;
        v.widgets.hovered.weak_bg_fill = p.button_hover;
        v.widgets.hovered.bg_fill = p.control_hover;
        v.widgets.active.weak_bg_fill = p.button_down;
        v.widgets.active.bg_fill = p.control_hover;
        v.selection.bg_fill = p.accent.gamma_multiply(0.55);
        v.selection.stroke.color = p.accent;
        v.widgets.inactive.fg_stroke.color = p.text;
        v.widgets.hovered.fg_stroke.color = p.text;
        v.widgets.active.fg_stroke.color = p.text;
        v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        v.widgets.active.bg_stroke = egui::Stroke::NONE;
        style.spacing.item_spacing = egui::vec2(4.0, 4.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.interaction.tooltip_delay = 0.4;
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    });
}

/// Segoe UI, so the app reads as a Windows app. Falls back to egui's bundled font.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(data) = read_font("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(data));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    ctx.set_fonts(fonts);
}

fn read_font(file: &str) -> Option<FontData> {
    std::fs::read(format!(r"C:\Windows\Fonts\{file}")).ok().map(FontData::from_owned)
}

/// Whether a font covers a code point.
type Covers = fn(u32) -> bool;

/// Windows' fonts for scripts Segoe UI doesn't cover, by the test that calls for each.
/// They're large (up to ~20 MB), so each loads only when a file name needs it.
const FALLBACKS: &[(&str, &str, Covers)] = &[
    ("yugothic", "YuGothM.ttc", |c| (0x3040..0x3100).contains(&c) || (0x31F0..0x3200).contains(&c)),
    ("yahei", "msyh.ttc", |c| {
        (0x2E80..0x3040).contains(&c)
            || (0x3100..0xA000).contains(&c)
            || (0xF900..0xFB00).contains(&c)
            || (0xFF00..0xFFF0).contains(&c)
    }),
    ("malgun", "malgun.ttf", |c| {
        (0x1100..0x1200).contains(&c) || (0xAC00..0xD7B0).contains(&c) || (0x3130..0x3190).contains(&c)
    }),
    ("nirmala", "Nirmala.ttc", |c| (0x0900..0x0E00).contains(&c)),
    ("symbol", "seguisym.ttf", |c| (0x2190..0x2C00).contains(&c) || (0x1F000..0x20000).contains(&c)),
];

/// Fonts loaded (or loading) so far; shared with the loader thread.
static FALLBACK_FONTS: std::sync::Mutex<Vec<(&'static str, Arc<FontData>)>> = std::sync::Mutex::new(Vec::new());
static REQUESTED: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

/// Make sure `text` can be drawn: fonts for any scripts Segoe UI lacks are read on a
/// background thread and installed when ready (a frame or two of placeholder boxes at most).
pub fn ensure_fonts_for(ctx: &egui::Context, text: &str) {
    let mut wanted = Vec::new();
    {
        let requested = REQUESTED.lock().unwrap();
        for c in text.chars() {
            for (name, file, test) in FALLBACKS {
                if test(c as u32) && !requested.contains(name) && !wanted.iter().any(|(n, _)| n == name) {
                    wanted.push((*name, *file));
                }
            }
        }
    }
    if wanted.is_empty() {
        return;
    }
    REQUESTED.lock().unwrap().extend(wanted.iter().map(|(n, _)| *n));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let loaded: Vec<_> =
            wanted.iter().filter_map(|(name, file)| Some((*name, Arc::new(read_font(file)?)))).collect();
        if loaded.is_empty() {
            return;
        }
        let mut all = FALLBACK_FONTS.lock().unwrap();
        all.extend(loaded);
        let mut fonts = FontDefinitions::default();
        if let Some(data) = read_font("segoeui.ttf") {
            fonts.font_data.insert("segoe".into(), Arc::new(data));
            fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
        }
        for (name, data) in all.iter() {
            fonts.font_data.insert((*name).into(), data.clone());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push((*name).into());
            }
        }
        crate::trace(&format!("fallback fonts ready: {:?}", all.iter().map(|(n, _)| *n).collect::<Vec<_>>()));
        ctx.set_fonts(fonts);
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_choice_round_trips() {
        for c in Choice::ALL {
            assert_eq!(Choice::from_storage(c.to_storage()), c);
        }
        assert_eq!(Choice::from_storage("anything else"), Choice::System);
        assert!(!Choice::Dark.resolve());
        assert!(Choice::Light.resolve());
    }

    #[test]
    fn fonts_are_chosen_by_script() {
        let wants = |text: &str| -> Vec<&str> {
            FALLBACKS.iter().filter(|(_, _, test)| text.chars().any(|c| test(c as u32))).map(|(n, ..)| *n).collect()
        };
        assert!(wants("café Ωmega Привет עברית.png").is_empty());
        assert_eq!(wants("画像.png"), ["yahei"]);
        assert_eq!(wants("がぞう 画像"), ["yugothic", "yahei"]);
        assert_eq!(wants("한국어"), ["malgun"]);
    }
}
