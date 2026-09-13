//! The accounts view: the root of the stack, one row per connected mailbox.

use eframe::egui::{CentralPanel, Context, RichText, ScrollArea, Sense, Vec2};

use crate::commands::Command;
use crate::nav::View;
use crate::state::PostApp;
use crate::ui::icons;

impl PostApp {
    pub fn accounts_view(&mut self, ctx: &Context) -> Vec<Command> {
        let mut out = Vec::new();
        let mut open: Option<(usize, String)> = None;
        let accounts = self.accounts.clone();
        let connecting = self.connecting;
        // One stop per account and "Add account" on the end; a list that just
        // got shorter brings the keyboard back onto it.
        let stops = self.account_stops();
        self.account_focus = self.account_focus.min(stops - 1);
        let focus = self.account_focus;
        let follow = std::mem::take(&mut self.follow_focus);
        let adding = focus == stops - 1;

        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            match accounts.as_deref() {
                None => self.empty(ui, "Reading the account list…"),
                Some([]) => self.empty(
                    ui,
                    "No accounts connected yet. Add one and its mail appears here.",
                ),
                Some(list) => {
                    ScrollArea::vertical().show(ui, |ui| {
                        for (i, email) in list.iter().enumerate() {
                            let selected = i == focus;
                            if account_row(ui, email, selected, selected && follow) {
                                open = Some((i, email.clone()));
                            }
                        }
                    });
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !connecting,
                        eframe::egui::Button::new("Add account").selected(adding),
                    )
                    .on_hover_text(
                        "Opens Google's own consent screen in your browser. \
                         Post never sees the password.",
                    )
                    .clicked()
                {
                    out.push(Command::AddAccount);
                }
                if connecting {
                    ui.spinner();
                    ui.label(RichText::new("Finish the sign-in in your browser…").weak());
                }
            });
        });

        if let Some((i, account)) = open {
            self.account_focus = i;
            self.goto(View::Inbox { account });
        }
        out
    }
}

/// One account, as a row wide enough to click anywhere on.
///
/// `follow` is the keyboard having just landed here: the row asks the list to
/// scroll it into view, which does nothing when it is already there.
fn account_row(ui: &mut eframe::egui::Ui, email: &str, selected: bool, follow: bool) -> bool {
    let height = 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    if follow {
        ui.scroll_to_rect(rect, None);
    }
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    }
    let color = ui.style().interact(&resp).text_color();
    let icon = eframe::egui::Rect::from_center_size(
        eframe::egui::Pos2::new(rect.min.x + 22.0, rect.center().y),
        Vec2::splat(20.0),
    );
    icons::draw(ui.painter(), icon, "account", color);
    ui.painter().text(
        eframe::egui::Pos2::new(rect.min.x + 44.0, rect.center().y),
        eframe::egui::Align2::LEFT_CENTER,
        email,
        eframe::egui::FontId::proportional(14.0),
        color,
    );
    resp.on_hover_text("Open this account's mail").clicked()
}
