//! The window around the views: which one is drawn, and the line at the bottom.

use eframe::egui::{Context, RichText, TopBottomPanel};

use crate::commands::Command;
use crate::nav::View;
use crate::state::PostApp;

impl PostApp {
    /// Draws the view on screen and returns whatever it asked for.
    pub fn view(&mut self, ctx: &Context, frame: &mut eframe::Frame) -> Vec<Command> {
        match self.nav.top().clone() {
            View::Accounts => self.accounts_view(ctx),
            View::Inbox { account } => self.inbox_view(ctx, &account),
            View::Detail { account, id } => self.detail_view(ctx, frame, &account, &id),
            View::Compose { account, .. } => self.compose_view(ctx, &account),
        }
    }

    /// One line: what is happening, or what went wrong, and where we are.
    pub fn status_bar(&mut self, ctx: &Context) {
        let where_ = match self.nav.top() {
            View::Accounts => "Accounts".to_string(),
            View::Inbox { account } => match &self.mailbox {
                Some(mailbox) => format!("{account} — {} unread", mailbox.unread()),
                None => account.clone(),
            },
            View::Detail { account, .. } => account.clone(),
            View::Compose { account, .. } => format!("{account} — replying"),
        };
        let status = self.status.clone();
        TopBottomPanel::bottom("post-status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(where_).weak());
                if !status.is_empty() {
                    ui.separator();
                    ui.label(status);
                }
            });
        });
    }

    /// A sentence for a view with nothing in it yet — never a blank rectangle.
    pub fn empty(&self, ui: &mut eframe::egui::Ui, sentence: &str) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(sentence).weak());
        });
    }
}
