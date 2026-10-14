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
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") else {
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    ctx.set_fonts(fonts);
}
