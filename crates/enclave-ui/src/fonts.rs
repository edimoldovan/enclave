//! Font setup.
//!
//! egui's bundled fonts cover Latin text and a small emoji set, but not the
//! geometric/technical symbols the ribbon uses for icons — those render as
//! empty boxes. Append any system symbol fonts we can find as fallbacks.

use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

/// Symbol fonts to append, in priority order. First existing file of each pair
/// wins; missing files are skipped, so this degrades quietly on any distro.
const CANDIDATES: &[&str] = &[
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

/// Installs symbol fallbacks. Returns the names of the fonts actually loaded.
pub fn install(ctx: &Context) -> Vec<String> {
    let mut fonts = FontDefinitions::default();
    let mut loaded = Vec::new();

    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| (*path).to_string());
        if fonts.font_data.contains_key(&name) {
            continue;
        }
        fonts
            .font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        // Fallbacks go last so the bundled font still wins for Latin text.
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push(name.clone());
            }
        }
        loaded.push(name);
    }

    ctx.set_fonts(fonts);
    loaded
}
