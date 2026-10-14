//! Settings and the keyboard-shortcuts sheet.

use super::*;

impl ViewerApp {
    pub(super) fn open_settings(&mut self) {
        self.settings_open = true;
        if self.editors.is_none() {
            self.editors = Some(editor::detect());
        }
    }

    pub(super) fn settings_ui(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let frame = egui::Frame::popup(&ctx.global_style()).fill(theme::WINDOW).inner_margin(20.0).corner_radius(10.0);
        let modal = egui::Modal::new(egui::Id::new("settings")).frame(frame).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Settings").size(18.0).strong());
            ui.add_space(14.0);
            ui.label(RichText::new("Image editor").strong());
            ui.label(
                RichText::new("Used by the Edit button. Any program that opens a file you give it will work.")
                    .color(theme::TEXT_DIM)
                    .small(),
            );
            ui.add_space(6.0);

            let mut choices = self.editors.clone().unwrap_or_default();
            choices.push(Editor::windows_default());
            if !choices.contains(&self.editor) {
                choices.insert(0, self.editor.clone());
            }
            // Buttons in here get a visible face; the toolbar's frameless look doesn't suit a dialog.
            ui.visuals_mut().widgets.inactive.weak_bg_fill = theme::BUTTON_HOVER;
            ui.visuals_mut().widgets.hovered.weak_bg_fill = theme::CONTROL_HOVER;
            for choice in choices {
                let selected = choice == self.editor;
                let response = ui.radio(selected, RichText::new(&choice.name).color(theme::TEXT));
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.add(egui::Label::new(RichText::new(choice.detail()).color(theme::TEXT_DIM).small()).truncate());
                });
                ui.add_space(2.0);
                if response.clicked() {
                    self.editor = choice;
                }
            }
            ui.add_space(4.0);
            if ui.button("Choose another program…").clicked() {
                let mut dialog = rfd::FileDialog::new().add_filter("Programs", &["exe"]);
                if let Some(pf) = std::env::var_os("ProgramFiles") {
                    dialog = dialog.set_directory(pf);
                }
                if let Some(exe) = dialog.pick_file() {
                    self.editor = Editor::from_exe(&exe);
                }
            }

            ui.add_space(16.0);
            ui.label(RichText::new("Viewing").strong());
            ui.add_space(4.0);
            ui.checkbox(&mut self.show_info, "Show the file info bar  (I)");
            ui.checkbox(&mut self.looping, "Loop animations  (L)");
            ui.checkbox(&mut self.gallery.open, "Show the gallery  (G)");

            ui.add_space(16.0);
            let mut show_shortcuts = false;
            let done = ui
                .horizontal(|ui| {
                    let version = format!("{} {}", register::APP_NAME, env!("CARGO_PKG_VERSION"));
                    ui.label(RichText::new(version).color(theme::TEXT_DIM).small());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let done = ui.button("  Done  ").clicked();
                        show_shortcuts = ui.button("Keyboard shortcuts").clicked();
                        done
                    })
                    .inner
                })
                .inner;
            (done, show_shortcuts)
        });
        let (done, show_shortcuts) = modal.inner;
        if done || show_shortcuts || modal.should_close() {
            self.settings_open = false;
        }
        if show_shortcuts {
            self.shortcuts_open = true;
        }
    }

    pub(super) fn shortcuts_ui(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        const SECTIONS: &[(&str, &[(&str, &str)])] = &[
            (
                "Browse",
                &[
                    ("→  ←", "Next / previous image"),
                    ("Home  End", "First / last image"),
                    ("Mouse back / forward", "Previous / next image"),
                    ("G", "Gallery"),
                    ("Ctrl+O", "Open a file"),
                    ("F5", "Reload"),
                ],
            ),
            (
                "View",
                &[
                    ("Wheel", "Zoom around the cursor"),
                    ("+  −", "Zoom in / out"),
                    ("0", "Fit to window"),
                    ("1 … 9", "100% … 900%"),
                    ("Drag", "Pan"),
                    ("Ctrl+.  Ctrl+,", "Rotate right / left (view only)"),
                    ("F11  Enter", "Full screen (or double-click)"),
                    ("I", "File info bar"),
                ],
            ),
            ("Animation", &[("Space", "Play / pause"), (",  .", "Previous / next frame"), ("L", "Loop on / off")]),
            (
                "File",
                &[
                    ("E", "Edit in your editor"),
                    ("Ctrl+C", "Copy image"),
                    ("Delete", "Move to Recycle Bin"),
                    ("Esc", "Leave full screen, or close"),
                ],
            ),
        ];
        let frame = egui::Frame::popup(&ctx.global_style()).fill(theme::WINDOW).inner_margin(20.0).corner_radius(10.0);
        let modal = egui::Modal::new(egui::Id::new("shortcuts")).frame(frame).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.label(RichText::new("Keyboard shortcuts").size(18.0).strong());
            ui.add_space(10.0);
            ui.columns(2, |cols| {
                for (i, (title, rows)) in SECTIONS.iter().enumerate() {
                    let ui = &mut cols[usize::from(i >= 2)];
                    ui.label(RichText::new(*title).strong());
                    ui.add_space(2.0);
                    egui::Grid::new(title).num_columns(2).spacing([14.0, 5.0]).show(ui, |ui| {
                        for (keys, action) in rows.iter() {
                            ui.label(RichText::new(*keys).color(theme::ACCENT));
                            ui.label(RichText::new(*action).color(theme::TEXT_DIM));
                            ui.end_row();
                        }
                    });
                    ui.add_space(12.0);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.visuals_mut().widgets.inactive.weak_bg_fill = theme::BUTTON_HOVER;
                ui.button("  Close  ").clicked()
            })
            .inner
        });
        if modal.inner || modal.should_close() {
            self.shortcuts_open = false;
        }
    }
}
