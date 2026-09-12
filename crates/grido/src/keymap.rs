//! Grido's keymap: the file it loads and the commands the chords name.
//!
//! The loader itself is shared (`enclave_ui::keymap`).

use crate::commands::Command;

/// Compiled-in fallback so the binary works with no config files at all.
pub const DEFAULT_KEYMAP: &str = include_str!("../keymap.toml");

pub type Keymap = enclave_ui::keymap::Keymap<Command>;

/// Loads the first keymap found in $GRIDO_KEYMAP, ./keymap.toml,
/// ~/.config/grido/keymap.toml, falling back to the embedded default.
/// Returns the keymap plus any warnings worth showing the user.
pub fn load() -> (Keymap, Vec<String>) {
    Keymap::load("grido", DEFAULT_KEYMAP)
}
