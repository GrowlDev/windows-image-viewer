//! Colours, fonts and egui style.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

pub const WINDOW: Color32 = Color32::from_rgb(27, 30, 35);
pub const CANVAS: Color32 = Color32::from_rgb(38, 42, 49);
pub const FULLSCREEN: Color32 = Color32::from_rgb(12, 13, 15);
pub const TEXT: Color32 = Color32::from_rgb(220, 223, 228);
pub const TEXT_DIM: Color32 = Color32::from_rgb(143, 149, 160);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(76, 81, 90);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(45, 49, 57);
pub const BUTTON_DOWN: Color32 = Color32::from_rgb(55, 60, 69);
pub const CONTROL: Color32 = Color32::from_rgb(52, 57, 66);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(66, 72, 83);
pub const DIVIDER: Color32 = Color32::from_rgb(48, 52, 60);
pub const ACCENT: Color32 = Color32::from_rgb(190, 204, 160);
pub const DANGER: Color32 = Color32::from_rgb(232, 128, 116);
pub const CHECKER: [Color32; 2] = [Color32::from_rgb(44, 48, 55), Color32::from_rgb(53, 57, 65)];

/// Title bar colour, matched to the window so the frame reads as one surface.
pub const CAPTION: [u8; 3] = [27, 30, 35];
pub const CAPTION_TEXT: [u8; 3] = [200, 204, 211];

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        let v = &mut style.visuals;
        v.panel_fill = WINDOW;
        v.window_fill = WINDOW;
        v.override_text_color = Some(TEXT);
        v.widgets.noninteractive.bg_stroke.color = DIVIDER;
        // Buttons: frameless until hovered. Checkboxes, radios and slider tracks: visible.
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = CONTROL;
        v.widgets.hovered.weak_bg_fill = BUTTON_HOVER;
        v.widgets.hovered.bg_fill = CONTROL_HOVER;
        v.widgets.active.weak_bg_fill = BUTTON_DOWN;
        v.widgets.active.bg_fill = CONTROL_HOVER;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
        v.selection.stroke.color = ACCENT;
        v.widgets.inactive.fg_stroke.color = TEXT;
        v.widgets.hovered.fg_stroke.color = TEXT;
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
