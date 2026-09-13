//! The shortcut viewer: every chord currently bound, and where it came from.

use eframe::egui::{Align2, Context, Grid, Key, RichText, ScrollArea, Window};

use crate::commands::Command;
use crate::state::PostApp;

impl PostApp {
    pub fn shortcuts_window(&mut self, ctx: &Context) {
        if !self.shortcuts {
            return;
        }
        let mut open = true;
        Window::new("Keyboard shortcuts")
            .collapsible(false)
            .resizable(true)
            .default_height(420.0)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("Loaded from: {}", self.keymap.source)).weak());
                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    Grid::new("post-shortcuts")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (shortcut, cmd) in &self.keymap.bindings {
                                ui.label(ui.ctx().format_shortcut(shortcut));
                                ui.label(cmd.label());
                                ui.end_row();
                            }
                        });
                });
            });
        // Escape closes it, and so does whatever the keymap says opens it —
        // asked of the keymap rather than named here, so a reader who rebinds
        // the viewer gets the same key back out of it.
        let toggles: Vec<_> = self
            .keymap
            .bindings
            .iter()
            .filter(|(_, cmd)| *cmd == Command::ShortcutHelp)
            .map(|(chord, _)| *chord)
            .collect();
        let dismissed = ctx.input_mut(|i| {
            i.key_pressed(Key::Escape) || toggles.iter().any(|chord| i.consume_shortcut(chord))
        });
        if !open || dismissed {
            self.shortcuts = false;
        }
    }
}
