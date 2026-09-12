//! Colors from the Omarchy theme, applied to the whole UI.
//!
//! Omarchy keeps the active theme at `~/.local/state/omarchy/current/theme/`.
//! We read `colors.toml` (flat keys, carries the theme's real `accent`) and
//! fall back to parsing `alacritty.toml` for older setups; machines without
//! Omarchy get a neutral dark scheme. Every surface derives from background/
//! foreground/accent mixes, so any theme just works — light ones included.
//!
//! A watcher polls the theme directory's identity and reloads in place, so a
//! theme switch restyles the app live along with the rest of the desktop.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Color32, Stroke};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub fg: Color32,
    pub accent: Color32,
}

fn theme_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".local/state/omarchy/current/theme"))
}

pub fn load() -> Palette {
    theme_dir().map(|d| load_from(&d)).unwrap_or_else(fallback)
}

fn load_from(dir: &Path) -> Palette {
    read_colors_toml(&dir.join("colors.toml"))
        .or_else(|| read_alacritty(&dir.join("alacritty.toml")))
        .unwrap_or_else(fallback)
}

fn fallback() -> Palette {
    Palette {
        bg: Color32::from_rgb(18, 20, 24),
        fg: Color32::from_rgb(200, 205, 212),
        accent: Color32::from_rgb(95, 135, 190),
    }
}

/// Omarchy's own palette file: flat `background` / `foreground` / `accent`.
fn read_colors_toml(path: &Path) -> Option<Palette> {
    let doc: toml::Table = std::fs::read_to_string(path).ok()?.parse().ok()?;
    let color = |key: &str| doc.get(key)?.as_str().and_then(hex_color);
    let bg = color("background")?;
    let fg = color("foreground")?;
    // Older themes may lack `accent`; ANSI bright blue is the usual stand-in.
    let accent = color("accent")
        .or_else(|| color("color12"))
        .or_else(|| color("color4"))
        .unwrap_or_else(|| fallback().accent);
    Some(Palette { bg, fg, accent })
}

/// The alacritty scheme every Omarchy theme also ships (viode reads this one).
fn read_alacritty(path: &Path) -> Option<Palette> {
    let doc: toml::Table = std::fs::read_to_string(path).ok()?.parse().ok()?;
    let colors = doc.get("colors")?;
    let get = |section: &str, key: &str| -> Option<Color32> {
        colors.get(section)?.get(key)?.as_str().and_then(hex_color)
    };
    Some(Palette {
        bg: get("primary", "background")?,
        fg: get("primary", "foreground")?,
        accent: get("bright", "blue")
            .or_else(|| get("normal", "blue"))
            .unwrap_or_else(|| fallback().accent),
    })
}

fn hex_color(s: &str) -> Option<Color32> {
    let hex = s.trim().strip_prefix('#')?;
    if hex.len() < 6 {
        return None;
    }
    let v = u32::from_str_radix(&hex[..6], 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

fn is_light(c: Color32) -> bool {
    // Perceived luminance; good enough to pick a base scheme.
    0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32 > 140.0
}

/// egui-wide visuals derived from the palette. Everything reads its colors
/// from these (panel fill, selection accent, grid-line stroke, header fill), so
/// this one function themes the ribbon, grid, dialogs and all.
pub fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if is_light(p.bg) {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.override_text_color = Some(p.fg);
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.extreme_bg_color = mix(p.bg, p.fg, 0.05);
    v.faint_bg_color = mix(p.bg, p.fg, 0.07);
    v.window_stroke = Stroke::new(1.0, mix(p.bg, p.fg, 0.35));
    v.selection.bg_fill = p.accent.gamma_multiply(0.40);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.widgets.noninteractive.bg_fill = p.bg;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, mix(p.fg, p.bg, 0.35));
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, mix(p.bg, p.fg, 0.14));
    v.widgets.inactive.bg_fill = mix(p.bg, p.fg, 0.10);
    v.widgets.inactive.weak_bg_fill = mix(p.bg, p.fg, 0.10);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.fg);
    v.widgets.hovered.bg_fill = mix(p.bg, p.accent, 0.25);
    v.widgets.hovered.weak_bg_fill = mix(p.bg, p.accent, 0.25);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.fg);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.bg_fill = mix(p.bg, p.accent, 0.40);
    v.widgets.active.weak_bg_fill = mix(p.bg, p.accent, 0.40);
    v.widgets.active.fg_stroke = Stroke::new(1.5, p.fg);
    v.widgets.open.bg_fill = mix(p.bg, p.accent, 0.20);
    v.widgets.open.weak_bg_fill = mix(p.bg, p.accent, 0.20);
    v
}

/// What identifies the current theme on disk. The `current/theme` path (or a
/// file inside it) changes identity when `omarchy theme set` runs.
fn identity(dir: &Path) -> (Option<PathBuf>, Option<SystemTime>, Option<SystemTime>) {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (
        std::fs::canonicalize(dir).ok(),
        mtime(&dir.join("colors.toml")),
        mtime(&dir.join("alacritty.toml")),
    )
}

/// Polls the theme cheaply and hands back a fresh palette when it changed —
/// the app follows a theme switch live, like the rest of the desktop.
pub struct ThemeWatcher {
    dir: Option<PathBuf>,
    seen: (Option<PathBuf>, Option<SystemTime>, Option<SystemTime>),
    last_check: Instant,
}

impl ThemeWatcher {
    pub fn new() -> ThemeWatcher {
        let dir = theme_dir();
        let seen = dir
            .as_deref()
            .map(identity)
            .unwrap_or((None, None, None));
        ThemeWatcher {
            dir,
            seen,
            last_check: Instant::now(),
        }
    }

    /// A fresh palette when the theme changed since last asked. Call it every
    /// frame; it touches the disk at most once a second.
    pub fn changed(&mut self, ctx: &egui::Context) -> Option<Palette> {
        // Keep a slow repaint scheduled so the poll still runs while the
        // window sits idle — an idle window should restyle too.
        ctx.request_repaint_after(Duration::from_secs(2));
        if self.last_check.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_check = Instant::now();
        let dir = self.dir.as_deref()?;
        let now = identity(dir);
        if now == self.seen {
            return None;
        }
        self.seen = now;
        Some(load_from(dir))
    }
}

impl Default for ThemeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_omarchy_colors_toml() {
        let dir = std::env::temp_dir().join("enclave-ui-theme-test-1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("colors.toml"),
            "accent = \"#4385BE\"\nforeground = \"#CECDC3\"\nbackground = \"#100F0F\"\n",
        )
        .unwrap();
        let p = load_from(&dir);
        assert_eq!(p.bg, Color32::from_rgb(0x10, 0x0F, 0x0F));
        assert_eq!(p.accent, Color32::from_rgb(0x43, 0x85, 0xBE));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn falls_back_to_alacritty_then_neutral() {
        let dir = std::env::temp_dir().join("enclave-ui-theme-test-2");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("alacritty.toml"),
            "[colors.primary]\nbackground = \"#121212\"\nforeground = \"#bebebe\"\n[colors.bright]\nblue = \"#f59e0b\"\n",
        )
        .unwrap();
        let p = load_from(&dir);
        assert_eq!(p.accent, Color32::from_rgb(0xf5, 0x9e, 0x0b));
        std::fs::remove_dir_all(&dir).ok();

        let empty = std::env::temp_dir().join("enclave-ui-theme-test-3");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(load_from(&empty), fallback());
        std::fs::remove_dir_all(&empty).ok();
    }

    #[test]
    fn light_backgrounds_get_the_light_base() {
        let light = Palette {
            bg: Color32::from_rgb(0xF2, 0xF0, 0xE5),
            fg: Color32::from_rgb(0x10, 0x0F, 0x0F),
            accent: Color32::from_rgb(0x20, 0x5E, 0xA6),
        };
        assert!(!visuals(&light).dark_mode);
        assert!(visuals(&fallback()).dark_mode);
    }

    #[test]
    fn hex_parsing_is_forgiving_about_length_only() {
        assert_eq!(hex_color("#ff0080"), Some(Color32::from_rgb(255, 0, 128)));
        assert_eq!(hex_color("#ff0080ff"), Some(Color32::from_rgb(255, 0, 128)));
        assert_eq!(hex_color("ff0080"), None);
        assert_eq!(hex_color("#ff00"), None);
    }
}
