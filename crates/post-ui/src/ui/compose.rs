//! The reply: who it goes to, what it is about, and the words themselves.
//!
//! The addresses come from the mail library — a reply answers everybody on the
//! message: its sender in To, everyone else in Cc — and both lines are then the
//! reader's to change, comma by comma. What they say when Send is pressed is
//! what leaves. The subject is shown and not editable: a reply is about what it
//! answers. The body has the keyboard from the moment this view opens.
//!
//! Under the editor is the message being answered — the attribution line and
//! its own text, quieter and scrollable. It is read, never written: the same
//! words go out under the reply behind `> `, and the mail library is what puts
//! them there.
//!
//! Two chords are taken before the editor sees them, because an editor with
//! the focus eats every key: Ctrl+Enter sends and Escape goes back. Nothing
//! else is claimed — the rest belongs to whoever is typing.

use eframe::egui::{
    Align, CentralPanel, Context, Key, Layout, Modifiers, RichText, ScrollArea, TextEdit, Ui, Vec2,
};

use crate::commands::Command;
use crate::state::PostApp;

/// How much of the panel the button row under the editor needs.
const BUTTONS: f32 = 34.0;

/// The box the label in front of each line sits in, so To, Cc and Subject all
/// start at the same place.
const LABEL: Vec2 = Vec2::new(54.0, 18.0);

impl PostApp {
    pub fn compose_view(&mut self, ctx: &Context, account: &str) -> Vec<Command> {
        let mut out = Vec::new();
        // Before the editor is built, so a focused text field cannot swallow
        // the only two keys that mean something here.
        ctx.input_mut(|input| {
            if input.consume_key(Modifiers::COMMAND, Key::Enter) {
                out.push(Command::Send);
            }
            if input.consume_key(Modifiers::NONE, Key::Escape) {
                out.push(Command::Back);
            }
        });

        let sending = self.sending;
        // The message being answered can be a mail's worth of text, so it
        // steps out of `self` for the frame rather than being copied into it.
        let draft = self.draft.take();
        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Reply").size(17.0).strong());
            ui.add_space(4.0);
            match &draft {
                Some(draft) => {
                    line(ui, "To", &mut self.to);
                    line(ui, "Cc", &mut self.cc);
                    field(ui, "Subject", &draft.subject);
                }
                None => {
                    field(ui, "From", account);
                    ui.label(RichText::new("Working out who this answers…").weak().size(13.0));
                }
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // The message being answered takes the lower part of the panel, so
            // there is always something on screen saying what this answers.
            let quoting = draft.as_ref().is_some_and(|d| !d.quote.trim().is_empty());
            let left = (ui.available_height() - BUTTONS).max(60.0);
            let room = Vec2::new(
                ui.available_width(),
                if quoting { (left * 0.55).max(60.0) } else { left },
            );
            let editor = ui.add_sized(
                room,
                TextEdit::multiline(&mut self.compose)
                    .hint_text("Write your reply")
                    .desired_width(f32::INFINITY),
            );
            // The body has the keyboard the moment this view opens, and keeps
            // it: nothing else here is typed into.
            if ui.memory(|m| m.focused().is_none()) {
                editor.request_focus();
            }

            // Straight under the editor, left-aligned with it.
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!sending, eframe::egui::Button::new("Send")).clicked() {
                    out.push(Command::Send);
                }
                if ui.button("Cancel").clicked() {
                    out.push(Command::Back);
                }
            });

            // What this answers, under the buttons: labels, so there is nothing
            // here to type into and nothing to take the keyboard.
            if let Some(draft) = draft.as_ref().filter(|_| quoting) {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .id_salt("quoted")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height().max(40.0))
                    .show(ui, |ui| {
                        ui.indent("quoted", |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&draft.attribution).weak().size(12.5));
                            ui.add_space(2.0);
                            ui.label(RichText::new(&draft.quote).weak().size(12.5));
                        });
                    });
            }
        });
        self.draft = draft;
        out
    }
}

/// One prefilled line above the editor: a label, then what it says.
fn field(ui: &mut Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        name_of(ui, name);
        ui.label(RichText::new(value).size(13.0));
    });
}

/// One line above the editor that is typed into: the addresses, comma
/// separated, as they will leave.
fn line(ui: &mut Ui, name: &str, value: &mut String) {
    ui.horizontal(|ui| {
        name_of(ui, name);
        ui.add(
            TextEdit::singleline(value)
                .hint_text("comma separated")
                .desired_width(f32::INFINITY),
        );
    });
}

/// The label in front of a line, in a box wide enough for all three so the
/// fields start in the same place.
fn name_of(ui: &mut Ui, name: &str) {
    ui.allocate_ui_with_layout(LABEL, Layout::left_to_right(Align::Center), |ui| {
        ui.label(RichText::new(name).weak().size(13.0));
    });
}
