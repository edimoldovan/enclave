//! The ribbon kit: the geometry and the reactions every ribbon control shares.
//!
//! An app supplies its own tabs, groups and commands. What it gets from here
//! is the strip's measurements — so icons line up and labels start at the same
//! height in every Enclave app — plus the hover/pressed/active treatment.

use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::color::rgb;

/// How a control draws an icon by name: the app's icon set, which falls back
/// to the shared one. Returns false when the name is not an icon, leaving the
/// caller to render it as text ("B", "I", "U", "$", …).
pub type IconFn = fn(&Painter, Rect, &str, Color32) -> bool;

// ----- shared ribbon-button geometry ---------------------------------------
//
// Every ribbon control — plain button, dropdown face, color picker — is drawn
// from the same two bands, so icons line up across the whole strip and every
// label starts at exactly the same height:
//
//   y  5..29   icon band: the icon centers in it, both axes
//   y 32..     label line: CENTER_TOP anchored, one shared font
//
pub const BTN_H: f32 = 48.0;
pub const ICON_TOP: f32 = 5.0;
pub const ICON_H: f32 = 24.0;
pub const LABEL_TOP: f32 = 32.0;
pub const LABEL_FONT: f32 = 10.5;
pub const BTN_ROUNDING: f32 = 5.0;

/// The band an icon centers in. `reserve` carves space off the bottom (the
/// color buttons put their stripe there).
pub fn icon_band(rect: Rect, reserve: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + ICON_TOP),
        Vec2::new(rect.width(), ICON_H - reserve),
    )
}

/// Sizes a button from its caption: wide enough for the text, never smaller
/// than a comfortable square.
pub fn button_rect(ui: &mut Ui, caption: &str) -> (Rect, Response) {
    let text_w = ui
        .painter()
        .layout_no_wrap(
            caption.to_owned(),
            FontId::proportional(LABEL_FONT),
            Color32::WHITE,
        )
        .size()
        .x;
    ui.allocate_exact_size(Vec2::new((text_w + 13.0).max(46.0), BTN_H), Sense::click())
}

/// Hover / pressed / active background, shared so every control reacts alike.
pub fn paint_button_bg(ui: &Ui, rect: Rect, resp: &Response, active: bool) {
    if active {
        let accent = ui.visuals().selection.stroke.color;
        ui.painter()
            .rect_filled(rect, BTN_ROUNDING, accent.gamma_multiply(0.28));
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(BTN_ROUNDING as u8),
            Stroke::new(1.0, accent),
            StrokeKind::Inside,
        );
    } else if resp.hovered() || resp.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, BTN_ROUNDING, ui.style().interact(resp).weak_bg_fill);
    }
}

/// Paints an icon centered in `band`: a vector glyph when `icons` has one,
/// otherwise the name as text ("B", "I", "U", "$", "Aa", …). Text is centered
/// rather than top-anchored, so letters sit exactly where the drawings do —
/// a font galley carries ascender padding that used to push them low.
pub fn paint_icon(
    painter: &Painter,
    band: Rect,
    icon: &str,
    color: Color32,
    size: f32,
    icons: IconFn,
) {
    let box_rect = Rect::from_center_size(band.center(), Vec2::splat(size));
    if !icons(painter, box_rect, icon, color) {
        // Letters have no descender, so a galley-centered glyph rides high;
        // nudge down to put the ink where the drawn icons put theirs.
        painter.text(
            band.center() + Vec2::new(0.0, size * 0.06),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(size),
            color,
        );
    }
}

/// Every ribbon label is drawn from this one line, so all of them start at
/// the same height regardless of what sits above.
pub fn paint_label(painter: &Painter, rect: Rect, text: &str, color: Color32) {
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + LABEL_TOP),
        Align2::CENTER_TOP,
        text,
        FontId::proportional(LABEL_FONT),
        color,
    );
}

/// Ribbon color button: glyph on top, the current color as a stripe, caption
/// with a dropdown arrow below. Clicking anywhere opens the palette.
pub fn color_button(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    color: Option<[u8; 3]>,
    icons: IconFn,
) -> Response {
    let caption = format!("{label} ▾");
    // The current color sits as a small swatch *beside* the caption, so the
    // icon above gets the full band and centers exactly like its neighbors.
    const SWATCH_W: f32 = 11.0;
    const SWATCH_GAP: f32 = 4.0;
    let label_font = FontId::proportional(LABEL_FONT);
    let text_w = ui
        .painter()
        .layout_no_wrap(caption.clone(), label_font.clone(), Color32::WHITE)
        .size()
        .x;
    let content_w = text_w + SWATCH_GAP + SWATCH_W;
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new((content_w + 13.0).max(46.0), BTN_H),
        Sense::click(),
    );
    paint_button_bg(ui, rect, &resp, false);
    let fg = ui.style().interact(&resp).text_color();
    paint_icon(ui.painter(), icon_band(rect, 0.0), icon, fg, 20.0, icons);

    let start_x = rect.center().x - content_w / 2.0;
    ui.painter().text(
        Pos2::new(start_x, rect.min.y + LABEL_TOP),
        Align2::LEFT_TOP,
        caption,
        label_font,
        fg,
    );
    // As tall as the caption's glyphs, sitting on the same line.
    let swatch = Rect::from_min_size(
        Pos2::new(start_x + text_w + SWATCH_GAP, rect.min.y + LABEL_TOP + 1.0),
        Vec2::new(SWATCH_W, 10.0),
    );
    match color {
        Some(c) => {
            ui.painter().rect_filled(swatch, 1.0, rgb(c));
        }
        None => {
            ui.painter().rect_stroke(
                swatch,
                CornerRadius::same(1),
                Stroke::new(1.0, fg),
                StrokeKind::Inside,
            );
        }
    }
    resp
}

/// Face of a ribbon dropdown: glyph on top, "Label ▾" below. Same footprint as
/// a plain ribbon button so groups stay visually even.
pub fn menu_face(ui: &mut Ui, icon: &str, label: &str, icons: IconFn) -> Response {
    let caption = format!("{label} ▾");
    let (rect, resp) = button_rect(ui, &caption);
    paint_button_bg(ui, rect, &resp, false);
    let fg = ui.style().interact(&resp).text_color();
    paint_icon(ui.painter(), icon_band(rect, 0.0), icon, fg, 20.0, icons);
    paint_label(ui.painter(), rect, &caption, fg);
    resp
}

/// Lays out a ribbon group: a row of controls with the caption centered below.
/// Free function so callers can borrow their app state mutably inside `add`.
pub fn ribbon_group_ui(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        let row = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                add(ui);
            })
            .response
            .rect;
        let (caption_rect, _) =
            ui.allocate_exact_size(Vec2::new(row.width(), 13.0), Sense::hover());
        ui.painter().text(
            Pos2::new(row.center().x, caption_rect.center().y),
            Align2::CENTER_CENTER,
            caption,
            FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
    });
    ui.separator();
}
