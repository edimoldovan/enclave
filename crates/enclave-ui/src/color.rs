//! Small color helpers shared by the widgets.

use eframe::egui::Color32;

/// Converts an engine color to an egui one.
pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Picks black or white text for legibility on the given background.
pub fn contrast_text(bg: [u8; 3]) -> Color32 {
    let luma = 0.299 * bg[0] as f32 + 0.587 * bg[1] as f32 + 0.114 * bg[2] as f32;
    if luma > 140.0 {
        Color32::from_rgb(20, 20, 20)
    } else {
        Color32::from_rgb(240, 240, 240)
    }
}
