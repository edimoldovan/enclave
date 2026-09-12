//! The standard color palette and its swatches.

use eframe::egui::{Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::color::rgb;

/// The standard color set; the palette's hue rows are tints/shades of these.
const PALETTE_HUES: [[u8; 3]; 10] = [
    [192, 0, 0],
    [255, 0, 0],
    [255, 192, 0],
    [255, 255, 0],
    [146, 208, 80],
    [0, 176, 80],
    [0, 176, 240],
    [0, 112, 192],
    [0, 32, 96],
    [112, 48, 160],
];

/// Top row of the palette: white through black.
const PALETTE_GREYS: [[u8; 3]; 10] = [
    [255, 255, 255],
    [242, 242, 242],
    [217, 217, 217],
    [191, 191, 191],
    [166, 166, 166],
    [128, 128, 128],
    [89, 89, 89],
    [64, 64, 64],
    [38, 38, 38],
    [0, 0, 0],
];

/// Blends toward white (`f > 0`) or black (`f < 0`).
fn tint(c: [u8; 3], f: f32) -> [u8; 3] {
    let target = if f > 0.0 { 255.0 } else { 0.0 };
    let k = f.abs();
    let mix = |v: u8| (v as f32 + (target - v as f32) * k).round().clamp(0.0, 255.0) as u8;
    [mix(c[0]), mix(c[1]), mix(c[2])]
}

/// The 10x5 grid: greys on top, then the hues lightened, plain and darkened.
pub fn palette_rows() -> Vec<[[u8; 3]; 10]> {
    let mut rows = vec![PALETTE_GREYS];
    for f in [0.6, 0.4, 0.0, -0.3] {
        let mut row = PALETTE_HUES;
        for c in row.iter_mut() {
            *c = tint(*c, f);
        }
        rows.push(row);
    }
    rows
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// One clickable color square in the palette popup.
pub fn swatch(ui: &mut Ui, color: [u8; 3]) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
    ui.painter().rect_filled(rect, 2.0, rgb(color));
    let stroke = if resp.hovered() {
        Stroke::new(1.5, ui.visuals().selection.bg_fill)
    } else {
        Stroke::new(1.0, Color32::from_gray(120))
    };
    ui.painter()
        .rect_stroke(rect, CornerRadius::same(2), stroke, StrokeKind::Inside);
    resp.on_hover_text(hex(color))
}
