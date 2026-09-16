//! The ribbon: one strip, and only the buttons the view on screen can act on.
//!
//! Post has four views and a handful of verbs, so there are no ribbon tabs to
//! sort them into — a tab strip over eight buttons is furniture. The geometry,
//! the hover treatment and the label line are the shared kit's, so the buttons
//! sit at exactly the height and weight Grido's do.

use eframe::egui::{Context, TopBottomPanel, Ui};

use enclave_ui::ribbon::{
    button_rect, icon_band, paint_button_bg, paint_icon, paint_label, ribbon_group_ui,
};

use crate::commands::Command;
use crate::nav::View;
use crate::state::PostApp;
use crate::ui::icons;

impl PostApp {
    /// Draws the strip and returns whatever was clicked.
    pub fn ribbon(&mut self, ctx: &Context) -> Vec<Command> {
        let mut out = Vec::new();
        TopBottomPanel::top("post-ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match self.nav.top() {
                    View::Accounts => {
                        self.group(
                            ui,
                            "Accounts",
                            &[
                                ("plus", "Add", Command::AddAccount),
                                ("refresh", "Refresh", Command::Refresh),
                            ],
                            &mut out,
                        );
                    }
                    View::Inbox { .. } => {
                        self.group(
                            ui,
                            "Message",
                            &[
                                ("mail_open", "Open", Command::Open),
                                ("trash", "Delete", Command::Delete),
                            ],
                            &mut out,
                        );
                        self.group(
                            ui,
                            "Mail",
                            &[
                                ("refresh", "Refresh", Command::Refresh),
                                ("account", "Accounts", Command::Accounts),
                                ("back", "Back", Command::Back),
                            ],
                            &mut out,
                        );
                    }
                    View::Detail { .. } => {
                        self.group(
                            ui,
                            "Message",
                            &[
                                ("reply", "Reply", Command::Reply),
                                ("trash", "Delete", Command::Delete),
                                ("mail", "Unread", Command::MarkUnread),
                            ],
                            &mut out,
                        );
                        self.group(
                            ui,
                            "Mail",
                            &[
                                ("refresh", "Refresh", Command::Refresh),
                                ("account", "Accounts", Command::Accounts),
                                ("back", "Back", Command::Back),
                            ],
                            &mut out,
                        );
                    }
                    View::Compose { .. } => {
                        self.group(
                            ui,
                            "Reply",
                            &[
                                ("send", "Send", Command::Send),
                                ("back", "Cancel", Command::Back),
                            ],
                            &mut out,
                        );
                        self.group(
                            ui,
                            "Mail",
                            &[("account", "Accounts", Command::Accounts)],
                            &mut out,
                        );
                    }
                }
                self.group(ui, "Help", &[("?", "Keys", Command::ShortcutHelp)], &mut out);
            });
        });
        out
    }

    /// A cluster of ribbon buttons with the group caption centered underneath.
    fn group(
        &self,
        ui: &mut Ui,
        caption: &str,
        items: &[(&str, &str, Command)],
        out: &mut Vec<Command>,
    ) {
        ribbon_group_ui(ui, caption, |ui| {
            for (icon, label, cmd) in items {
                self.button(ui, icon, label, *cmd, out);
            }
        });
    }

    /// Ribbon button: large icon on top, label below, both centered.
    fn button(&self, ui: &mut Ui, icon: &str, label: &str, cmd: Command, out: &mut Vec<Command>) {
        let active = cmd == Command::ShortcutHelp && self.shortcuts;
        let (rect, resp) = button_rect(ui, label);
        paint_button_bg(ui, rect, &resp, active);
        let color = ui.style().interact(&resp).text_color();
        paint_icon(ui.painter(), icon_band(rect, 0.0), icon, color, 20.0, icons::draw);
        paint_label(ui.painter(), rect, label, color);
        let resp = match self.keymap.shortcut_for(cmd) {
            Some(sc) => resp.on_hover_text(format!(
                "{}  ({})",
                cmd.label(),
                ui.ctx().format_shortcut(&sc)
            )),
            None => resp.on_hover_text(cmd.label()),
        };
        if resp.clicked() {
            out.push(cmd);
        }
    }
}
