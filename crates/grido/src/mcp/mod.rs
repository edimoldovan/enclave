//! Model Context Protocol support: the spreadsheet's half of it.
//!
//! The transport (socket, stdio shim, JSON-RPC) lives in the `enclave` host;
//! what lives here is what the assistant can actually do to a workbook — the
//! tool surface, the templates, the skill — plus the request type the UI
//! thread drains.
//!
//! The server runs inside the GUI so an assistant edits the workbook the user
//! is looking at, and every change is visible as it happens. Starting the app
//! any normal way (desktop launcher included) is all that is required — there
//! is no flag and no separate daemon.

use std::sync::mpsc::Sender;

use serde_json::Value;

pub mod register;
pub mod skill;
pub mod templates;
pub mod tools;

/// One tool call plus the channel its result goes back on.
///
/// The host's bridge sends these from a socket thread; `GridoApp::update`
/// drains them on the UI thread, where the `Engine` lives.
pub struct Call {
    pub tool: String,
    pub args: Value,
    pub reply: Sender<Result<Value, String>>,
}
