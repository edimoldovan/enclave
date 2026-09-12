//! Key chords bound to commands, loaded from a TOML file.
//!
//! The mechanism is shared; the bindings are not. An app passes its own name
//! and its own compiled-in default, and gets back chords resolved against its
//! own command list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui::{Key, KeyboardShortcut, Modifiers};
use serde::Deserialize;

use crate::command::CommandId;

#[derive(Deserialize)]
struct KeymapFile {
    bindings: BTreeMap<String, String>,
}

pub struct Keymap<C> {
    pub bindings: Vec<(KeyboardShortcut, C)>,
    pub source: String,
}

impl<C: CommandId> Keymap<C> {
    /// Loads the first keymap found in $<APP>_KEYMAP, ./keymap.toml,
    /// ~/.config/<app>/keymap.toml, falling back to `default` (the app's
    /// compiled-in keymap, so the binary works with no config files at all).
    /// Returns the keymap plus any warnings worth showing the user.
    pub fn load(app: &str, default: &str) -> (Keymap<C>, Vec<String>) {
        let mut warnings = Vec::new();
        for path in candidate_paths(app) {
            if !path.is_file() {
                continue;
            }
            let source = path.display().to_string();
            match std::fs::read_to_string(&path) {
                Ok(text) => match parse(&text) {
                    Ok((bindings, mut warns)) => {
                        warnings.append(&mut warns);
                        return (Keymap { bindings, source }, warnings);
                    }
                    Err(e) => warnings.push(format!("{source}: {e} (using default keymap)")),
                },
                Err(e) => warnings.push(format!("{source}: {e}")),
            }
        }
        let (bindings, mut warns) = parse(default).expect("embedded keymap.toml must parse");
        warnings.append(&mut warns);
        (
            Keymap {
                bindings,
                source: "built-in".to_string(),
            },
            warnings,
        )
    }

    /// First shortcut bound to `cmd`, for menu labels.
    pub fn shortcut_for(&self, cmd: C) -> Option<KeyboardShortcut> {
        self.bindings
            .iter()
            .find(|(_, c)| *c == cmd)
            .map(|(sc, _)| *sc)
    }
}

fn candidate_paths(app: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(p) = std::env::var(format!("{}_KEYMAP", app.to_ascii_uppercase())) {
        paths.push(PathBuf::from(p));
    }
    paths.push(PathBuf::from("keymap.toml"));
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(format!(".config/{app}/keymap.toml")));
    }
    paths
}

fn parse<C: CommandId>(text: &str) -> Result<(Vec<(KeyboardShortcut, C)>, Vec<String>), String> {
    let file: KeymapFile = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut bindings = Vec::new();
    let mut warnings = Vec::new();
    for (chord, id) in &file.bindings {
        let Some(shortcut) = parse_chord(chord) else {
            warnings.push(format!("keymap: cannot parse chord \"{chord}\""));
            continue;
        };
        let Some(cmd) = C::from_id(id) else {
            warnings.push(format!("keymap: unknown command \"{id}\""));
            continue;
        };
        bindings.push((shortcut, cmd));
    }
    Ok((bindings, warnings))
}

pub fn parse_chord(chord: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in chord.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            "super" | "cmd" | "win" => mods.command = true,
            other => {
                // Key names are case-sensitive in egui ("F2", "ArrowUp"), so
                // retry with the original spelling before giving up.
                key = Key::from_name(part.trim()).or_else(|| {
                    let mut upper = other.to_string();
                    upper.get_mut(0..1).map(|c| c.make_ascii_uppercase());
                    Key::from_name(&upper)
                });
            }
        }
    }
    Some(KeyboardShortcut::new(mods, key?))
}
