//! Vector icons, drawn with the painter.
//!
//! Unicode symbol coverage varies wildly between systems — several glyphs a
//! ribbon wants render as empty boxes. These are drawn with the painter
//! instead, so they look the same everywhere and need no font at all.
//!
//! The set here is the one every Enclave app shares. An app draws its own
//! glyphs with the same [`Pen`], so the whole suite keeps one stroke weight,
//! one optical size and one set of proportions.
//!
//! `draw` returns false for names it doesn't know, letting the caller fall
//! back to its own icons, or to rendering the name as text.

use std::cell::RefCell;

use eframe::egui::emath::TSTransform;
use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// Draws `name` centered in `rect`. Returns false if there is no such icon.
///
/// Centering is *optical*: the ink the icon actually produces is measured and
/// that bounding box is what lands in the middle. Each icon draws with its own
/// internal margins, so centering the coordinate box (as this used to do)
/// left some icons visibly high or low, with uneven gaps to the label below.
/// Undersized glyphs are also gently scaled toward a shared visual size.
pub fn draw(painter: &Painter, rect: Rect, name: &str, color: Color32) -> bool {
    let Some(mut shapes) = build(rect, name, color) else {
        return false;
    };
    center_optically(&mut shapes, rect);
    painter.add(Shape::Vec(shapes));
    true
}

/// Moves (and gently rescales) shapes so their combined ink is centered in
/// `rect` at a consistent visual size.
pub fn center_optically(shapes: &mut [Shape], rect: Rect) {
    let mut bbox = Rect::NOTHING;
    for shape in shapes.iter() {
        bbox = bbox.union(shape.visual_bounding_rect());
    }
    if !bbox.is_positive() {
        return;
    }
    let target = rect.width().min(rect.height()) * 0.82;
    let extent = bbox.width().max(bbox.height()).max(1.0);
    let scale = (target / extent).clamp(0.85, 1.3);
    let translation = rect.center().to_vec2() - bbox.center().to_vec2() * scale;
    let t = TSTransform::new(translation, scale);
    for shape in shapes.iter_mut() {
        shape.transform(t);
    }
}

/// The drawing primitives every icon is made of.
///
/// Coordinates are fractions of a square box centered in the icon's rect, so
/// an icon keeps its proportions whatever size it is asked for. Strokes scale
/// with the box: `thin` for outlines, `thick` for the one or two strokes an
/// icon leans on.
pub struct Pen {
    box_rect: Rect,
    side: f32,
    color: Color32,
    shapes: RefCell<Vec<Shape>>,
}

impl Pen {
    pub fn new(rect: Rect, color: Color32) -> Pen {
        // Work in a square box so icons keep their proportions.
        let side = rect.width().min(rect.height());
        Pen {
            box_rect: Rect::from_center_size(rect.center(), Vec2::splat(side)),
            side,
            color,
            shapes: RefCell::new(Vec::new()),
        }
    }

    /// The square box's side, for radii and stroke widths.
    pub fn side(&self) -> f32 {
        self.side
    }

    pub fn color(&self) -> Color32 {
        self.color
    }

    pub fn thin(&self) -> Stroke {
        Stroke::new((self.side * 0.075).max(1.0), self.color)
    }

    pub fn thick(&self) -> Stroke {
        Stroke::new((self.side * 0.11).max(1.3), self.color)
    }

    /// A point at fractions `x`, `y` of the box.
    pub fn p(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(
            self.box_rect.min.x + x * self.box_rect.width(),
            self.box_rect.min.y + y * self.box_rect.height(),
        )
    }

    pub fn push(&self, s: Shape) {
        self.shapes.borrow_mut().push(s);
    }

    pub fn seg(&self, pts: [Pos2; 2], stroke: Stroke) {
        self.push(Shape::line_segment(pts, stroke));
    }

    pub fn circle_stroke(&self, c: Pos2, r: f32, stroke: Stroke) {
        self.push(Shape::circle_stroke(c, r, stroke));
    }

    pub fn circle_filled(&self, c: Pos2, r: f32) {
        self.push(Shape::circle_filled(c, r, self.color));
    }

    pub fn line(&self, a: (f32, f32), b: (f32, f32)) {
        self.seg([self.p(a.0, a.1), self.p(b.0, b.1)], self.thin());
    }

    pub fn rect_outline(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.push(Shape::closed_line(
            vec![
                self.p(x0, y0),
                self.p(x1, y0),
                self.p(x1, y1),
                self.p(x0, y1),
            ],
            self.thin(),
        ));
    }

    pub fn rect_filled(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.push(Shape::rect_filled(
            Rect::from_min_max(self.p(x0, y0), self.p(x1, y1)),
            0.0,
            self.color,
        ));
    }

    /// Arrowhead pointing in a cardinal direction, centered on (x, y).
    pub fn arrow_head(&self, x: f32, y: f32, dx: f32, dy: f32, size: f32) {
        let tip = self.p(x, y);
        let back = Vec2::new(-dx, -dy) * size * self.box_rect.width();
        let side_v = Vec2::new(-dy, dx) * size * 0.62 * self.box_rect.width();
        self.push(Shape::convex_polygon(
            vec![tip, tip + back + side_v, tip + back - side_v],
            self.color,
            Stroke::NONE,
        ));
    }

    pub fn into_shapes(self) -> Vec<Shape> {
        self.shapes.into_inner()
    }
}

/// Builds a shared icon's shapes in `rect`'s coordinate box, or None for a
/// name this set doesn't carry.
pub fn build(rect: Rect, name: &str, color: Color32) -> Option<Vec<Shape>> {
    let pen = Pen::new(rect, color);
    {
        let side = pen.side();
        let thin = pen.thin();
        let thick = pen.thick();
        let p = |x: f32, y: f32| pen.p(x, y);
        let push = |s: Shape| pen.push(s);
        let seg = |pts: [Pos2; 2], stroke: Stroke| pen.seg(pts, stroke);
        let circle_stroke = |c: Pos2, r: f32, stroke: Stroke| pen.circle_stroke(c, r, stroke);
        let circle_filled = |c: Pos2, r: f32| pen.circle_filled(c, r);
        let line = |a: (f32, f32), b: (f32, f32)| pen.line(a, b);
        let rect_outline = |x0: f32, y0: f32, x1: f32, y1: f32| pen.rect_outline(x0, y0, x1, y1);
        let rect_filled = |x0: f32, y0: f32, x1: f32, y1: f32| pen.rect_filled(x0, y0, x1, y1);
        let arrow_head =
            |x: f32, y: f32, dx: f32, dy: f32, size: f32| pen.arrow_head(x, y, dx, dy, size);

        match name {
        // ----- clipboard -----
        "cut" => {
            line((0.28, 0.12), (0.66, 0.66));
            line((0.72, 0.12), (0.34, 0.66));
            circle_stroke(p(0.30, 0.80), side * 0.11, thin);
            circle_stroke(p(0.70, 0.80), side * 0.11, thin);
        }
        "copy" => {
            rect_outline(0.16, 0.10, 0.62, 0.68);
            rect_outline(0.38, 0.32, 0.84, 0.90);
        }
        "paste" => {
            rect_outline(0.20, 0.18, 0.80, 0.90);
            rect_filled(0.36, 0.08, 0.64, 0.22);
        }

        // ----- alignment -----
        "align-left" | "align-center" | "align-right" => {
            let rows = [0.22f32, 0.40, 0.58, 0.76];
            for (i, y) in rows.iter().enumerate() {
                let full = i % 2 == 0;
                let w = if full { 0.68 } else { 0.44 };
                let (x0, x1) = match name {
                    "align-left" => (0.16, 0.16 + w),
                    "align-right" => (0.84 - w, 0.84),
                    _ => (0.5 - w / 2.0, 0.5 + w / 2.0),
                };
                seg(
                    [p(x0, *y), p(x1, *y)],
                    Stroke::new((side * 0.085).max(1.0), color),
                );
            }
        }
        "wrap" => {
            line((0.16, 0.28), (0.84, 0.28));
            line((0.16, 0.52), (0.68, 0.52));
            push(Shape::line(
                vec![p(0.68, 0.52), p(0.80, 0.52), p(0.80, 0.72), p(0.40, 0.72)],
                thin,
            ));
            arrow_head(0.34, 0.72, -1.0, 0.0, 0.13);
        }

        // ----- clear -----
        "clear" => {
            // Eraser: a rounded block with a wipe stroke.
            rect_outline(0.16, 0.30, 0.72, 0.70);
            line((0.60, 0.22), (0.88, 0.50));
            line((0.76, 0.78), (0.90, 0.92));
            line((0.90, 0.78), (0.76, 0.92));
        }

        // ----- data -----
        "sort" => {
            // Two opposed arrows, the usual sort idiom.
            line((0.32, 0.86), (0.32, 0.24));
            arrow_head(0.32, 0.12, 0.0, -1.0, 0.16);
            line((0.68, 0.14), (0.68, 0.76));
            arrow_head(0.68, 0.88, 0.0, 1.0, 0.16);
        }
        "sort-asc" | "sort-desc" => {
            let down = name == "sort-desc";
            let widths = [0.30f32, 0.44, 0.58];
            for (i, w) in widths.iter().enumerate() {
                let y = 0.24 + i as f32 * 0.24;
                let w = if down { widths[2 - i] } else { *w };
                seg([p(0.14, y), p(0.14 + w, y)], thin);
            }
        }
        "filter" => {
            push(Shape::convex_polygon(
                vec![p(0.12, 0.18), p(0.88, 0.18), p(0.58, 0.52), p(0.42, 0.52)],
                color,
                Stroke::NONE,
            ));
            push(Shape::convex_polygon(
                vec![p(0.42, 0.56), p(0.58, 0.56), p(0.55, 0.88), p(0.45, 0.80)],
                color,
                Stroke::NONE,
            ));
        }
        "find" => {
            circle_stroke(p(0.44, 0.44), side * 0.26, thin);
            seg([p(0.63, 0.63), p(0.86, 0.86)], thick);
        }
        "chart" => {
            line((0.14, 0.12), (0.14, 0.86));
            line((0.14, 0.86), (0.90, 0.86));
            rect_filled(0.26, 0.52, 0.42, 0.86);
            rect_filled(0.48, 0.30, 0.64, 0.86);
            rect_filled(0.70, 0.44, 0.86, 0.86);
        }

        // ----- view -----
        "zoom-in" | "zoom-out" => {
            circle_stroke(p(0.44, 0.44), side * 0.26, thin);
            seg([p(0.63, 0.63), p(0.86, 0.86)], thick);
            line((0.30, 0.44), (0.58, 0.44));
            if name == "zoom-in" {
                line((0.44, 0.30), (0.44, 0.58));
            }
        }
        "zoom-reset" => {
            circle_stroke(p(0.50, 0.52), side * 0.30, thin);
            arrow_head(0.50, 0.20, 1.0, 0.0, 0.15);
        }

        // ----- documents -----
        "duplicate" => {
            rect_outline(0.14, 0.10, 0.58, 0.70);
            rect_outline(0.40, 0.30, 0.86, 0.90);
        }
        "rename" => {
            push(Shape::closed_line(
                vec![p(0.18, 0.72), p(0.66, 0.14), p(0.84, 0.30), p(0.36, 0.88)],
                thin,
            ));
            line((0.18, 0.72), (0.36, 0.88));
        }
        "trash" => {
            line((0.16, 0.24), (0.84, 0.24));
            line((0.40, 0.24), (0.40, 0.14));
            line((0.60, 0.24), (0.60, 0.14));
            line((0.40, 0.14), (0.60, 0.14));
            push(Shape::closed_line(
                vec![p(0.24, 0.30), p(0.76, 0.30), p(0.68, 0.90), p(0.32, 0.90)],
                thin,
            ));
        }
        "prev" | "next" => {
            let dir = if name == "next" { 1.0 } else { -1.0 };
            let x = if name == "next" { 0.72 } else { 0.28 };
            arrow_head(x, 0.50, dir, 0.0, 0.26);
            line((0.5 - dir * 0.34, 0.50), (0.5 + dir * 0.10, 0.50));
        }

        // ----- history / file -----
        "undo" | "redo" => {
            let mirror = name == "redo";
            let fx = |x: f32| if mirror { 1.0 - x } else { x };
            push(Shape::line(
                vec![
                    p(fx(0.20), 0.34),
                    p(fx(0.58), 0.34),
                    p(fx(0.76), 0.52),
                    p(fx(0.62), 0.80),
                ],
                thin,
            ));
            arrow_head(fx(0.20), 0.34, if mirror { 1.0 } else { -1.0 }, 0.0, 0.17);
        }
        "save" => {
            rect_outline(0.14, 0.14, 0.86, 0.86);
            rect_filled(0.32, 0.14, 0.68, 0.40);
            rect_outline(0.28, 0.56, 0.72, 0.86);
        }
        "open" => {
            push(Shape::closed_line(
                vec![p(0.10, 0.78), p(0.10, 0.26), p(0.40, 0.26), p(0.50, 0.38), p(0.84, 0.38), p(0.84, 0.78)],
                thin,
            ));
        }
        "fill-color" => {
            // Paint bucket: a tilted body with a drip.
            push(Shape::closed_line(
                vec![p(0.16, 0.52), p(0.50, 0.18), p(0.84, 0.52), p(0.50, 0.86)],
                thin,
            ));
            line((0.50, 0.10), (0.50, 0.24));
            circle_filled(p(0.86, 0.74), side * 0.08);
        }
        "new-doc" => {
            push(Shape::closed_line(
                vec![p(0.22, 0.10), p(0.62, 0.10), p(0.80, 0.30), p(0.80, 0.90), p(0.22, 0.90)],
                thin,
            ));
            line((0.62, 0.10), (0.62, 0.30));
            line((0.62, 0.30), (0.80, 0.30));
        }
            _ => return None,
        }
    }
    Some(pen.into_shapes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[&str] = &[
        "cut", "copy", "paste", "save", "undo", "redo", "clear", "sort", "sort-asc", "sort-desc",
        "filter", "find", "chart", "zoom-in", "zoom-out", "zoom-reset", "duplicate", "rename",
        "trash", "prev", "next", "align-left", "align-center", "align-right", "wrap", "open",
        "new-doc", "fill-color",
    ];

    #[test]
    fn every_icon_lands_optically_centered() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::splat(20.0));
        for name in SAMPLE {
            let mut shapes =
                build(rect, name, Color32::WHITE).unwrap_or_else(|| panic!("missing icon {name}"));
            center_optically(&mut shapes, rect);
            let mut bbox = Rect::NOTHING;
            for s in &shapes {
                bbox = bbox.union(s.visual_bounding_rect());
            }
            let c = bbox.center() - rect.center();
            assert!(
                c.x.abs() < 0.5 && c.y.abs() < 0.5,
                "{name} ink is off-center by {c:?}"
            );
            assert!(
                bbox.width().max(bbox.height()) >= rect.width() * 0.6,
                "{name} came out too small: {:?}",
                bbox.size()
            );
        }
    }

    #[test]
    fn unknown_names_build_nothing() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
        assert!(build(rect, "no-such-icon", Color32::WHITE).is_none());
    }
}
