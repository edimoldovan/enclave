//! The conversation: every message on it, oldest at the top, newest at the
//! bottom, and the open one's body directly under its own line.
//!
//! A message is one line — who wrote it and when — and clicking it (or Enter on
//! it) opens it in place: the body appears between that line and the next.
//! Only ever one is open, because the body is not an egui widget: it is a
//! native view laid over a rectangle of this panel, so there is one rectangle
//! to give away and one message that can have it. What happens here is that
//! the lines are drawn, the room the open body wants is measured where it
//! falls, and that rectangle is handed over and left empty.
//!
//! A native view is not clipped by anything egui draws, so the rectangle handed
//! over is only ever the part of it the conversation is actually showing: the
//! room it wants, cut to the scroll area's viewport, and the view taken off
//! screen altogether once its message has scrolled out.

use eframe::egui::{
    Align2, CentralPanel, Context, FontId, Pos2, Rect, RichText, ScrollArea, Sense, Vec2,
};

use crate::client::Message;
use crate::commands::Command;
use crate::state::PostApp;
use crate::web::{Placed, Px};

/// One message's line in the conversation.
const LINE_H: f32 = 30.0;
const LINE_X: f32 = 8.0;
const LINE_DATE_W: f32 = 116.0;

/// How tall an open HTML body opens. A native pane has no height of its own to
/// lay out by, so the height is guessed here: never more than what remains of
/// the viewport around it, and never less than a line or two. ROW_GUESS is
/// roughly one collapsed attribution line.
const ROW_GUESS: f32 = 30.0;
const BODY_MIN: f32 = 80.0;
/// The page the body is rendered in, as `web::page` writes it: 14px text on
/// 1.5 lines, 14px of padding all round. The guess is made in those numbers so
/// it says what the pane will do — and in the page's pixels rather than this
/// window's points, because they are not the same length. See `body_height`.
const BODY_FONT: f32 = 14.0;
const BODY_LINE: f32 = 21.0;
const BODY_PAD: f32 = 14.0;
/// How much of the message is laid out to guess by. Anything longer than this
/// has already passed the cap several times over, so reading further only
/// costs a bigger layout every frame.
const BODY_SAMPLE: usize = 4000;

impl PostApp {
    pub fn detail_view(
        &mut self,
        ctx: &Context,
        frame: &mut eframe::Frame,
        account: &str,
        id: &str,
    ) -> Vec<Command> {
        let out = Vec::new();
        if self.conversation.is_none() {
            CentralPanel::default().show(ctx, |ui| {
                self.empty(ui, "Opening the conversation…");
            });
            return out;
        }

        // egui hands the panel a closure, and the closure wants all of `self`.
        // The two big fields step out for the duration and go straight back,
        // rather than being copied every frame: a mail body is measured in
        // hundreds of kilobytes.
        let conversation = self.conversation.take().expect("a conversation");
        let page = self.body.take();
        let ready = self.ready;
        // Whatever the page keys asked for since the last frame, taken rather
        // than read: only the text body can be scrolled from here. The HTML
        // pane is a native view with a keyboard of its own — wry offers no way
        // to scroll it from outside, and a message is not a program to run one
        // in — so over that pane the keys that scroll it are WebKit's. Only the
        // chords bound to Back are taken back; see `web::chord`.
        let mut scroll = std::mem::take(&mut self.body_scroll);
        // A body in the native pane scrolls inside WebKit, not in this scroll
        // area — the arrows' points are handed to it as a synthesized wheel.
        if scroll != 0.0 && self.web.placed() == Placed::Pane {
            self.web.scroll(scroll * ctx.pixels_per_point());
            scroll = 0.0;
        }
        let follow = std::mem::take(&mut self.follow_focus);
        let open = conversation.opened().map(|message| message.id.clone());
        let key = format!("{account}/{}", open.as_deref().unwrap_or(id));
        let ppp = ctx.pixels_per_point();
        // This open message's own picture verdict, which travels with its page.
        let images = page.as_ref().is_some_and(|page| page.images);
        let density = self.web.density(ppp);
        let mut tapped: Option<usize> = None;

        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(if conversation.subject.is_empty() {
                    "(no subject)"
                } else {
                    &conversation.subject
                })
                .size(17.0)
                .strong(),
            );
            if conversation.messages.len() > 1 {
                ui.label(
                    RichText::new(format!("{} messages", conversation.messages.len()))
                        .size(12.0)
                        .weak(),
                );
            }
            ui.add_space(4.0);
            ui.separator();

            // The conversation itself: one line per message, oldest first, and
            // the open one's body between its line and the next.
            if conversation.opened().is_none() {
                self.web.hide();
            }
            // The most an open body may take: what is left of the viewport
            // after its own line and the collapsed lines below it — a lone
            // message fills the window, a mid-thread one leaves the rest
            // visible. Measured out here, where the height is the
            // conversation's own viewport rather than the unbounded room
            // inside a scroll area.
            let after = conversation
                .open
                .map_or(0, |i| conversation.messages.len() - 1 - i);
            let cap = (ui.available_height() - (after as f32 + 1.0) * ROW_GUESS - 8.0)
                .max(BODY_MIN);
            ScrollArea::vertical()
                .id_salt("post-conversation")
                .show(ui, |ui| {
                    // What the page keys asked for, which here moves the
                    // conversation: egui moves the content rather than the
                    // window onto it, so it goes in negated.
                    if scroll != 0.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, -scroll));
                    }
                    for (i, message) in conversation.messages.iter().enumerate() {
                        let on = conversation.open == Some(i);
                        let here = conversation.focus == i;
                        if attribution(ui, message, on, here, here && follow) {
                            tapped = Some(i);
                        }
                        if !on {
                            continue;
                        }
                        if !message.attachments.is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "Attachments: {}",
                                    message.attachments.join(", ")
                                ))
                                .size(12.5)
                                .weak(),
                            );
                        }
                        // The room the body would take, measured before
                        // anything is put in it: a pane is handed this
                        // rectangle, and a text body simply flows on instead.
                        let room = ui.available_rect_before_wrap();
                        let tall = body_height(ui, message, images, room.width(), cap, density);
                        let want = Rect::from_min_size(room.min, Vec2::new(room.width(), tall));
                        // A native pane draws over everything egui puts on this
                        // window, so while the shortcut viewer is up the body
                        // steps aside — keeping its room, so nothing moves.
                        if self.shortcuts && self.web.placed() == Placed::Pane {
                            self.web.set_shown(false);
                            ui.allocate_space(want.size());
                            continue;
                        }
                        let Some(page) = &page else {
                            self.web.hide();
                            text_body(ui, message);
                            continue;
                        };
                        // Only what the conversation is showing of it: a native
                        // view is clipped by nothing, so a body scrolled past
                        // the viewport is placed on what is left of it, and a
                        // body scrolled out of it altogether comes off screen.
                        let seen = want.intersect(ui.clip_rect());
                        if seen.height() < 1.0 || seen.width() < 1.0 {
                            self.web.set_shown(false);
                            ui.allocate_space(want.size());
                            continue;
                        }
                        match self.web.show(frame, ready, &key, &page.html, px(seen, ppp)) {
                            // The webview owns that rectangle: hold the room and
                            // draw nothing in it.
                            Placed::Pane => {
                                self.web.set_shown(true);
                                ui.allocate_space(want.size());
                            }
                            Placed::Beside => {
                                ui.label(
                                    RichText::new("The formatted message is in its own window.")
                                        .size(12.0)
                                        .weak(),
                                );
                                text_body(ui, message);
                            }
                            Placed::Nowhere => text_body(ui, message),
                        }
                    }
                });
            if conversation.opened().is_none() {
                ui.add_space(6.0);
                ui.label(RichText::new("Nothing is open — press Enter on a message.").weak());
            }
        });
        self.conversation = Some(conversation);
        self.body = page;
        // A line tapped is the keyboard landing there and Enter on it: the
        // mouse and the key press are the same thing happening.
        if let Some(i) = tapped {
            if let Some(conversation) = &mut self.conversation {
                conversation.focus = i;
            }
            self.toggle_message();
        }
        out
    }
}

/// One message's line: who wrote it and when, with the open one carried by the
/// selection colour and unread by weight. Returns true when it was clicked.
fn attribution(
    ui: &mut eframe::egui::Ui,
    message: &Message,
    open: bool,
    focused: bool,
    follow: bool,
) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), LINE_H), Sense::click());
    if follow {
        ui.scroll_to_rect(rect, None);
    }
    let visuals = ui.visuals();
    if focused {
        ui.painter().rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.faint_bg_color);
    }
    let color = if open || message.unread {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let weak = visuals.weak_text_color();

    let who = Rect::from_min_max(
        Pos2::new(rect.min.x + LINE_X, rect.min.y),
        Pos2::new((rect.max.x - LINE_DATE_W).max(rect.min.x + LINE_X + 1.0), rect.max.y),
    );
    let font = FontId::proportional(13.0);
    let painter = ui.painter().with_clip_rect(who);
    let at = Pos2::new(who.min.x, rect.center().y);
    painter.text(at, Align2::LEFT_CENTER, &message.from, font.clone(), color);
    if message.unread {
        painter.text(
            at + Vec2::new(0.35, 0.0),
            Align2::LEFT_CENTER,
            &message.from,
            font,
            color,
        );
    }
    // Where the body of an unopened message would be: its first line, quiet,
    // so the conversation reads as a conversation rather than a column of
    // names.
    if !open {
        let said: String = message
            .text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .chars()
            .take(140)
            .collect();
        let wrote = painter
            .layout_no_wrap(message.from.clone(), FontId::proportional(13.0), color)
            .size()
            .x;
        painter.text(
            Pos2::new(who.min.x + wrote + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            said,
            FontId::proportional(12.0),
            weak,
        );
    }
    ui.painter().text(
        Pos2::new(rect.max.x - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        &message.date,
        FontId::proportional(12.0),
        weak,
    );
    resp.on_hover_text(if open {
        "Close this message"
    } else {
        "Open this message"
    })
    .clicked()
}

/// How much room to hand an open body, when the pane itself cannot be asked.
///
/// The message's own extracted text is laid out at the pane's width, and the
/// lines it takes are counted in the page's line height: a note of two
/// sentences then opens as two sentences rather than as half the window. A
/// body with pictures in it is not its text, so that one takes the cap.
///
/// All of that is measured in the page's pixels — the pane is placed in
/// physical pixels, so its width in them is `density` times this window's — and
/// the answer is turned back into points at the end. Measuring the page in
/// points instead makes every line of it a third taller than it is on a screen
/// that draws a point as more than one pixel, and a note of two sentences then
/// opens as half the window after all.
fn body_height(
    ui: &eframe::egui::Ui,
    message: &Message,
    images: bool,
    width: f32,
    cap: f32,
    density: f32,
) -> f32 {
    if images {
        return cap;
    }
    let wrap = (width * density - 2.0 * BODY_PAD).max(1.0);
    let text: String = message.text.chars().take(BODY_SAMPLE).collect();
    let lines = ui
        .painter()
        .layout(
            text,
            FontId::proportional(BODY_FONT),
            ui.visuals().text_color(),
            wrap,
        )
        .rows
        .len() as f32;
    let tall = lines * BODY_LINE + 2.0 * BODY_PAD;
    (tall / density).clamp(BODY_MIN, cap)
}

/// A rectangle of this window in the physical pixels a webview is placed in.
fn px(rect: Rect, ppp: f32) -> Px {
    Px {
        x: (rect.min.x * ppp) as f64,
        y: (rect.min.y * ppp) as f64,
        w: (rect.width() * ppp) as f64,
        h: (rect.height() * ppp) as f64,
    }
}

/// The body as the mail library extracted it — what a plain-text message says,
/// and what an HTML one says once it is stripped. It is egui text, so it lays
/// out where it falls and the conversation scrolls over it.
fn text_body(ui: &mut eframe::egui::Ui, message: &Message) {
    ui.add_space(6.0);
    if message.text.trim().is_empty() {
        ui.label(RichText::new("This message has no body.").weak());
    } else {
        ui.label(RichText::new(&message.text).size(13.5));
    }
    ui.add_space(6.0);
}
