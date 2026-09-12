//! Grido's own ribbon icons.
//!
//! The shared set (`enclave_ui::icons`) carries what every Enclave app needs —
//! cut, copy, save, find, zoom, the arrows. What is left here is spreadsheet
//! vocabulary: rows and columns, frozen panes, grid lines, a new sheet. They
//! are drawn with the same [`Pen`], so they sit at the same weight and size as
//! the shared ones.
//!
//! `draw` returns false for names neither set knows, letting the caller fall
//! back to rendering the name as text (used for "B", "I", "U", "$", "%", …).

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape};

use enclave_ui::icons::{Pen, center_optically};

/// Draws `name` centered in `rect`. Returns false if there is no such icon.
/// Grido's own glyphs win; anything else comes from the shared set.
pub fn draw(painter: &Painter, rect: Rect, name: &str, color: Color32) -> bool {
    let Some(mut shapes) =
        build(rect, name, color).or_else(|| enclave_ui::icons::build(rect, name, color))
    else {
        return false;
    };
    center_optically(&mut shapes, rect);
    painter.add(Shape::Vec(shapes));
    true
}

/// Builds a Grido icon's shapes in `rect`'s coordinate box, or None for a
/// name the spreadsheet set doesn't carry.
fn build(rect: Rect, name: &str, color: Color32) -> Option<Vec<Shape>> {
    let pen = Pen::new(rect, color);
    {
        let side = pen.side();
        let thick = pen.thick();
        let p = |x: f32, y: f32| pen.p(x, y);
        let push = |s: Shape| pen.push(s);
        let seg = |pts: [Pos2; 2], stroke| pen.seg(pts, stroke);
        let circle_filled = |c: Pos2, r: f32| pen.circle_filled(c, r);
        let line = |a: (f32, f32), b: (f32, f32)| pen.line(a, b);
        let rect_outline = |x0: f32, y0: f32, x1: f32, y1: f32| pen.rect_outline(x0, y0, x1, y1);
        let rect_filled = |x0: f32, y0: f32, x1: f32, y1: f32| pen.rect_filled(x0, y0, x1, y1);
        let arrow_head =
            |x: f32, y: f32, dx: f32, dy: f32, size: f32| pen.arrow_head(x, y, dx, dy, size);

        match name {
        // ----- fill -----
        "fill" => {
            line((0.5, 0.12), (0.5, 0.66));
            arrow_head(0.5, 0.80, 0.0, 1.0, 0.17);
            line((0.20, 0.92), (0.80, 0.92));
        }

        // ----- structure -----
        "insert" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            line((0.12, 0.50), (0.88, 0.50));
            line((0.50, 0.62), (0.50, 0.86));
            line((0.38, 0.74), (0.62, 0.74));
        }
        "delete" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            line((0.12, 0.50), (0.88, 0.50));
            line((0.36, 0.70), (0.64, 0.70));
        }
        "resize" => {
            line((0.16, 0.50), (0.84, 0.50));
            arrow_head(0.10, 0.50, -1.0, 0.0, 0.16);
            arrow_head(0.90, 0.50, 1.0, 0.0, 0.16);
            line((0.16, 0.20), (0.16, 0.80));
            line((0.84, 0.20), (0.84, 0.80));
        }

        // ----- names -----
        "name" => {
            push(Shape::closed_line(
                vec![p(0.10, 0.30), p(0.62, 0.30), p(0.88, 0.50), p(0.62, 0.70), p(0.10, 0.70)],
                pen.thin(),
            ));
            circle_filled(p(0.26, 0.50), side * 0.06);
        }

        // ----- styles -----
        "cond-fmt" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            rect_filled(0.14, 0.14, 0.86, 0.36);
            line((0.12, 0.60), (0.88, 0.60));
            line((0.50, 0.36), (0.50, 0.88));
        }

        // ----- view -----
        "freeze" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            seg([p(0.12, 0.42), p(0.88, 0.42)], thick);
            seg([p(0.42, 0.12), p(0.42, 0.88)], thick);
        }
        "freeze-top" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            rect_filled(0.14, 0.14, 0.86, 0.36);
            seg([p(0.12, 0.38), p(0.88, 0.38)], thick);
        }
        "freeze-first" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            rect_filled(0.14, 0.14, 0.36, 0.86);
            seg([p(0.38, 0.12), p(0.38, 0.88)], thick);
        }
        "unfreeze" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            line((0.24, 0.24), (0.76, 0.76));
            line((0.76, 0.24), (0.24, 0.76));
        }
        "gridlines" => {
            rect_outline(0.12, 0.12, 0.88, 0.88);
            line((0.12, 0.37), (0.88, 0.37));
            line((0.12, 0.63), (0.88, 0.63));
            line((0.37, 0.12), (0.37, 0.88));
            line((0.63, 0.12), (0.63, 0.88));
        }

        // ----- sheets -----
        "sheet-new" => {
            rect_outline(0.14, 0.10, 0.70, 0.90);
            line((0.78, 0.52), (0.78, 0.86));
            line((0.62, 0.69), (0.94, 0.69));
        }
            _ => return None,
        }
    }
    Some(pen.into_shapes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

    /// The spreadsheet glyphs, plus a few from the shared set to prove the
    /// ribbon still reaches them through this door.
    const SAMPLE: &[&str] = &[
        "fill", "insert", "delete", "resize", "name", "cond-fmt", "freeze", "freeze-top",
        "freeze-first", "unfreeze", "gridlines", "sheet-new", "cut", "copy", "paste", "save",
    ];

    fn shapes_for(rect: Rect, name: &str) -> Option<Vec<Shape>> {
        build(rect, name, Color32::WHITE)
            .or_else(|| enclave_ui::icons::build(rect, name, Color32::WHITE))
    }

    #[test]
    fn every_icon_lands_optically_centered() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::splat(20.0));
        for name in SAMPLE {
            let mut shapes =
                shapes_for(rect, name).unwrap_or_else(|| panic!("missing icon {name}"));
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
        assert!(shapes_for(rect, "no-such-icon").is_none());
    }
}
