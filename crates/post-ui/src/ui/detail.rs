//! The message: who it is from, what it is about, and the body itself.
//!
//! The body is the message's own HTML wherever a webview can be put, and the
//! extracted text wherever one cannot. The webview is not an egui widget — it
//! is a native view laid over this panel's rectangle — so what happens here is
//! that the rectangle is measured, handed over, and left empty.

use eframe::egui::{CentralPanel, Context, RichText, ScrollArea, Vec2};

use crate::commands::Command;
use crate::state::PostApp;
use crate::web::{Placed, Px};

impl PostApp {
    pub fn detail_view(
        &mut self,
        ctx: &Context,
        frame: &mut eframe::Frame,
        account: &str,
        id: &str,
    ) -> Vec<Command> {
        let out = Vec::new();
        if self.message.is_none() {
            CentralPanel::default().show(ctx, |ui| {
                self.empty(ui, "Opening the message…");
            });
            return out;
        }

        // egui hands the panel a closure, and the closure wants all of `self`.
        // The two big fields step out for the duration and go straight back,
        // rather than being copied every frame: a mail body is measured in
        // hundreds of kilobytes.
        let message = self.message.take().expect("a message");
        let page = self.body.take();
        let ready = self.ready;
        // Whatever the arrow keys asked for since the last frame, taken rather
        // than read: only the text body can be scrolled from here. The HTML
        // pane is a native view with a keyboard of its own — wry offers no way
        // to scroll it from outside, and a message is not a program to run one
        // in — so over that pane the arrows that scroll it are WebKit's. Only
        // the chords bound to Back are taken back; see `web::chord`.
        let scroll = std::mem::take(&mut self.body_scroll);
        let key = format!("{account}/{id}");
        let ppp = ctx.pixels_per_point();

        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(if message.subject.is_empty() {
                    "(no subject)"
                } else {
                    &message.subject
                })
                .size(17.0)
                .strong(),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&message.from).size(13.0));
                ui.separator();
                ui.label(RichText::new(&message.date).size(13.0).weak());
            });
            if !message.attachments.is_empty() {
                ui.label(
                    RichText::new(format!("Attachments: {}", message.attachments.join(", ")))
                        .size(12.5)
                        .weak(),
                );
            }
            ui.add_space(6.0);
            ui.separator();

            let area = ui.available_rect_before_wrap();
            // A native pane draws over everything egui puts on this window, so
            // while the shortcut viewer is up the body steps aside.
            if self.shortcuts && self.web.placed() == Placed::Pane {
                self.web.set_shown(false);
                ui.allocate_space(area.size());
                return;
            }
            let placed = match &page {
                Some(page) => self.web.show(
                    frame,
                    ready,
                    &key,
                    page,
                    Px {
                        x: (area.min.x * ppp) as f64,
                        y: (area.min.y * ppp) as f64,
                        w: (area.width() * ppp) as f64,
                        h: (area.height() * ppp) as f64,
                    },
                ),
                None => {
                    self.web.hide();
                    Placed::Nowhere
                }
            };

            match placed {
                // The webview owns this rectangle: hold the space and draw
                // nothing under it.
                Placed::Pane => {
                    self.web.set_shown(true);
                    ui.allocate_space(area.size());
                }
                Placed::Beside => {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("The formatted message is in its own window.")
                            .size(12.0)
                            .weak(),
                    );
                    text_body(ui, &message.text, scroll);
                }
                Placed::Nowhere => text_body(ui, &message.text, scroll),
            }
        });
        self.message = Some(message);
        self.body = page;
        out
    }
}

/// The body as the mail library extracted it — what a plain-text message says,
/// and what an HTML one says once it is stripped.
///
/// `scroll` is what the arrow keys asked for, in points and positive downwards.
/// egui moves the content rather than the window onto it, so it goes in negated.
fn text_body(ui: &mut eframe::egui::Ui, text: &str, scroll: f32) {
    ui.add_space(6.0);
    if text.trim().is_empty() {
        ui.label(RichText::new("This message has no body.").weak());
        return;
    }
    ScrollArea::vertical().show(ui, |ui| {
        if scroll != 0.0 {
            ui.scroll_with_delta(Vec2::new(0.0, -scroll));
        }
        ui.label(RichText::new(text).size(13.5));
    });
}
