//! Grido — a fast native spreadsheet.
//!
//! It is one of the UIs the `enclave` binary can show; the binary is a thin
//! host around this library, so integration tests can drive the engine
//! directly.
//!
//! The look and feel — theme, fonts, ribbon geometry, shared icons — comes
//! from `enclave-ui`; what lives here is the spreadsheet itself.

pub mod app;
pub mod commands;
pub mod engine;
pub mod mcp;
pub mod keymap;
pub mod state;
pub mod ui;
