//! The Enclave design system: the parts of the look and feel every Enclave app
//! shares.
//!
//! Nothing here knows about any one app. An app brings its own command list,
//! its own ribbon tabs and its own glyphs, and gets the theme, the fonts, the
//! keymap loader, the ribbon geometry and the drawing primitives from here, so
//! two Enclave apps look and behave like one product.

pub mod color;
pub mod command;
pub mod fonts;
pub mod icons;
pub mod keymap;
pub mod palette;
pub mod ribbon;
pub mod theme;
