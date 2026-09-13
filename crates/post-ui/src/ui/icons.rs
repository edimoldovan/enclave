//! Post's own ribbon icons.
//!
//! The shared set (`enclave_ui::icons`) carries what every Enclave app needs —
//! the trash, find, save, the arrows. What is left here is mail vocabulary: an
//! envelope, a person, a plus, a back arrow, a reload. They are drawn with the
//! same [`Pen`], so they sit at the same weight and size as the shared ones.
//!
//! `draw` returns false for names neither set knows, letting the caller fall
//! back to rendering the name as text.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape};

use enclave_ui::icons::{center_optically, Pen};

/// Draws `name` centered in `rect`. Returns false if there is no such icon.
/// Post's own glyphs win; anything else comes from the shared set.
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

/// Builds a Post icon's shapes, or None for a name the mail set doesn't carry.
fn build(rect: Rect, name: &str, color: Color32) -> Option<Vec<Shape>> {
    let pen = Pen::new(rect, color);
    {
        let thin = pen.thin();
        let thick = pen.thick();
        let p = |x: f32, y: f32| pen.p(x, y);
        let push = |s: Shape| pen.push(s);
        let line = |a: (f32, f32), b: (f32, f32)| pen.line(a, b);

        match name {
            // An envelope: the flap is what makes it read as mail at 20px.
            "mail" => {
                pen.rect_outline(0.12, 0.24, 0.88, 0.76);
                push(Shape::line(
                    vec![p(0.12, 0.24), p(0.5, 0.54), p(0.88, 0.24)],
                    thin,
                ));
            }
            // The same, opened: a message that has been read.
            "mail_open" => {
                push(Shape::line(
                    vec![p(0.12, 0.78), p(0.12, 0.42), p(0.5, 0.18), p(0.88, 0.42), p(0.88, 0.78)],
                    thin,
                ));
                line((0.12, 0.78), (0.88, 0.78));
                push(Shape::line(
                    vec![p(0.12, 0.42), p(0.5, 0.66), p(0.88, 0.42)],
                    thin,
                ));
            }
            // One person: the account rows and the accounts button.
            "account" => {
                pen.circle_stroke(p(0.5, 0.33), pen.side() * 0.17, thin);
                push(Shape::line(
                    vec![p(0.16, 0.84), p(0.22, 0.62), p(0.78, 0.62), p(0.84, 0.84)],
                    thin,
                ));
            }
            // Back: an arrow pointing left.
            "back" => {
                line((0.22, 0.5), (0.84, 0.5));
                push(Shape::line(vec![p(0.46, 0.26), p(0.20, 0.5), p(0.46, 0.74)], thick));
            }
            // Reload: an almost-closed circle with a head on the loose end.
            "refresh" => {
                let (cx, cy, r) = (0.5f32, 0.52f32, 0.32f32);
                let from = std::f32::consts::PI * -0.62;
                let sweep = std::f32::consts::PI * 1.72;
                let arc: Vec<Pos2> = (0..=32)
                    .map(|i| {
                        let a = from + sweep * i as f32 / 32.0;
                        p(cx + r * a.cos(), cy + r * a.sin())
                    })
                    .collect();
                push(Shape::line(arc, thin));
                // The head sits on the open end, pointing the way round.
                pen.arrow_head(cx + r * from.cos(), cy + r * from.sin() - 0.02, 0.55, -0.83, 0.20);
            }
            // Plus: connecting an account.
            "plus" => {
                push(Shape::line_segment([p(0.5, 0.18), p(0.5, 0.82)], thick));
                push(Shape::line_segment([p(0.18, 0.5), p(0.82, 0.5)], thick));
            }
            // A filled dot: the unread marker on a row.
            "unread" => {
                pen.circle_filled(p(0.5, 0.5), pen.side() * 0.22);
            }
            _ => return None,
        }
    }
    Some(pen.into_shapes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Rect;

    /// Every name the ribbon asks for draws something — a missing glyph is an
    /// empty box on screen, which is exactly what these exist to avoid.
    #[test]
    fn every_icon_the_ribbon_uses_is_drawn() {
        let rect = Rect::from_min_size(Pos2::ZERO, eframe::egui::Vec2::splat(24.0));
        for name in [
            "mail", "mail_open", "account", "back", "refresh", "plus", "unread",
            // These come from the shared set.
            "trash", "open",
        ] {
            let shapes = build(rect, name, Color32::WHITE)
                .or_else(|| enclave_ui::icons::build(rect, name, Color32::WHITE));
            assert!(shapes.is_some_and(|s| !s.is_empty()), "no icon called {name}");
        }
        assert!(build(rect, "sendmail", Color32::WHITE).is_none());
    }
}
