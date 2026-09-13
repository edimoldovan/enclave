//! The inbox: one page of one account's mail, newest first.
//!
//! A row is painted rather than assembled out of widgets, the way Grido paints
//! its grid: the whole row is one click target, with the trash a zone at its
//! right-hand end. Unread is carried by weight — the app's font has one face,
//! so the glyphs are struck twice a third of a pixel apart, which is what a
//! bold face does — and by a dot in the margin, never by a badge.

use eframe::egui::{
    Align2, CentralPanel, Color32, Context, FontId, Painter, Pos2, Rect, ScrollArea, Sense, Vec2,
};

use crate::client::Row;
use crate::commands::Command;
use crate::state::PostApp;
use crate::ui::icons;

const ROW_H: f32 = 46.0;
const DOT_X: f32 = 16.0;
const TEXT_X: f32 = 30.0;
const SENDER_W: f32 = 210.0;
const DATE_W: f32 = 116.0;
const TRASH_W: f32 = 36.0;

/// How close to the end of the list the wheel has to get before the page after
/// it is asked for. About two rows: near enough to be arriving, far enough that
/// it is already there when the reader gets to the bottom.
const NEAR_END: f32 = ROW_H * 2.0;

/// What a click on a row asked for.
enum Hit {
    Open,
    Delete,
}

impl PostApp {
    pub fn inbox_view(&mut self, ctx: &Context, account: &str) -> Vec<Command> {
        let out = Vec::new();
        let rows = self.rows().to_vec();
        let focus = self.focus;
        let follow = std::mem::take(&mut self.follow_focus);
        let loading = self.mailbox.is_none();
        let paging = self.paging;
        let mut hit: Option<(Hit, String)> = None;
        let mut focused: Option<usize> = None;
        let mut near_end = false;

        CentralPanel::default().show(ctx, |ui| {
            if loading {
                self.empty(ui, "Fetching this mailbox…");
                return;
            }
            if rows.is_empty() {
                self.empty(ui, "Nothing in this mailbox.");
                return;
            }
            let out = ScrollArea::vertical().show(ui, |ui| {
                for (i, row) in rows.iter().enumerate() {
                    let selected = i == focus;
                    if let Some(what) = message_row(ui, row, selected, selected && follow) {
                        focused = Some(i);
                        hit = Some((what, row.id.clone()));
                    }
                }
                if paging {
                    loading_row(ui);
                }
            });
            // The wheel arriving at the end of the list is the ask for the page
            // after it — but only where there was something to scroll.
            let seen = out.state.offset.y + out.inner_rect.height();
            near_end = out.content_size.y > out.inner_rect.height()
                && out.content_size.y - seen < NEAR_END;
        });

        if near_end {
            self.load_more();
        }
        if let Some(i) = focused {
            self.focus = i;
        }
        match hit {
            Some((Hit::Open, id)) => self.open(account, &id),
            Some((Hit::Delete, id)) => self.delete(account, &id),
            None => {}
        }
        out
    }
}

/// One message's row. Returns what was clicked, if anything.
///
/// `follow` is the keyboard having just landed here: the row asks the list to
/// scroll it into view, which does nothing when it is already there.
fn message_row(ui: &mut eframe::egui::Ui, row: &Row, focused: bool, follow: bool) -> Option<Hit> {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    if follow {
        ui.scroll_to_rect(rect, None);
    }
    let trash = Rect::from_min_max(
        Pos2::new(rect.max.x - TRASH_W, rect.min.y),
        Pos2::new(rect.max.x, rect.max.y),
    );
    let on_trash = resp.hover_pos().is_some_and(|at| trash.contains(at));

    let visuals = ui.visuals();
    if focused {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.faint_bg_color);
    }
    let fg = visuals.text_color();
    let strong = visuals.strong_text_color();
    let weak = visuals.weak_text_color();
    let accent = visuals.selection.stroke.color;

    if row.unread {
        ui.painter()
            .circle_filled(Pos2::new(rect.min.x + DOT_X, rect.center().y), 4.0, accent);
    }

    let (name_color, body_color) = if row.unread {
        (strong, fg)
    } else {
        (fg, weak)
    };
    let top = rect.center().y - 9.0;
    let bottom = rect.center().y + 9.0;

    let sender = Rect::from_min_max(
        Pos2::new(rect.min.x + TEXT_X, rect.min.y),
        Pos2::new(rect.min.x + TEXT_X + SENDER_W, rect.max.y),
    );
    weighted(
        &ui.painter().with_clip_rect(sender),
        Pos2::new(sender.min.x, top),
        &row.from,
        13.5,
        name_color,
        row.unread,
    );

    let subject_right = trash.min.x - DATE_W - 10.0;
    let subject = Rect::from_min_max(
        Pos2::new(rect.min.x + TEXT_X, rect.min.y),
        Pos2::new(subject_right.max(rect.min.x + TEXT_X + 1.0), rect.max.y),
    );
    weighted(
        &ui.painter().with_clip_rect(subject),
        Pos2::new(subject.min.x, bottom),
        if row.subject.is_empty() {
            "(no subject)"
        } else {
            &row.subject
        },
        13.0,
        body_color,
        row.unread,
    );

    ui.painter().text(
        Pos2::new(trash.min.x - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        &row.date,
        FontId::proportional(12.0),
        weak,
    );

    icons::draw(
        ui.painter(),
        Rect::from_center_size(trash.center(), Vec2::splat(17.0)),
        "trash",
        if on_trash { accent } else { weak },
    );

    let resp = resp.on_hover_text(if on_trash {
        "Move this message to the trash"
    } else {
        "Open this message"
    });
    if !resp.clicked() {
        return None;
    }
    Some(if on_trash { Hit::Delete } else { Hit::Open })
}

/// The line at the end of the list while the page after it is on the wire.
/// Quiet on purpose: it is a note that more is coming, not a row to click.
fn loading_row(ui: &mut eframe::egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.min.x + TEXT_X, rect.center().y),
        Align2::LEFT_CENTER,
        "Loading…",
        FontId::proportional(12.5),
        ui.visuals().weak_text_color(),
    );
}

/// Text, optionally bold. The bundled font has one face, so weight is a second
/// strike a fraction of a pixel across — which is what a bold face is.
fn weighted(
    painter: &Painter,
    at: Pos2,
    text: &str,
    size: f32,
    color: Color32,
    bold: bool,
) {
    let font = FontId::proportional(size);
    painter.text(at, Align2::LEFT_CENTER, text, font.clone(), color);
    if bold {
        painter.text(at + Vec2::new(0.35, 0.0), Align2::LEFT_CENTER, text, font, color);
    }
}
