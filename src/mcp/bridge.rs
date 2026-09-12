//! The link between the socket thread and the UI thread.
//!
//! A tool call arrives on a socket thread but must touch the `Engine`, which
//! lives on the UI thread inside the eframe app. Requests are sent over a
//! channel with a one-shot reply channel attached; `GridoApp::update` drains
//! them each frame. `Context::request_repaint` wakes the loop, so calls land
//! promptly even when the window is idle.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// One tool call plus the channel its result goes back on. The UI that drains
/// them owns the type; this is only the wire between the two threads.
pub use grido::mcp::Call;

/// Sender half, shared with socket threads.
static TX: OnceLock<Mutex<Sender<Call>>> = OnceLock::new();
/// Set once the app has a repaint handle.
static WAKE: OnceLock<Mutex<Option<eframe::egui::Context>>> = OnceLock::new();

/// Creates the channel. Returns the receiver for the UI thread to drain.
pub fn install() -> Receiver<Call> {
    let (tx, rx) = channel();
    let _ = TX.set(Mutex::new(tx));
    rx
}

/// Gives the bridge a handle it can use to wake the UI thread.
pub fn set_wake(ctx: eframe::egui::Context) {
    match WAKE.get() {
        Some(slot) => {
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(ctx);
            }
        }
        None => {
            let _ = WAKE.set(Mutex::new(Some(ctx)));
        }
    }
}

/// Brings this process's window to the front.
///
/// Used when a second launch hands its file over: the person typed a command
/// and expects a window, so the one that already exists comes forward.
pub fn front() {
    if let Some(slot) = WAKE.get() {
        if let Ok(guard) = slot.lock() {
            if let Some(ctx) = guard.as_ref() {
                use eframe::egui::ViewportCommand;
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                ctx.request_repaint();
            }
        }
    }
}

/// Runs a tool on the UI thread and waits for its result.
///
/// Errors are strings so they can be handed straight to the model as an
/// `isError` result rather than killing the connection.
pub fn call(tool: &str, args: Value) -> Result<Value, String> {
    let (reply, answer) = channel();
    let tx = TX
        .get()
        .ok_or_else(|| "bridge not installed".to_string())?
        .lock()
        .map_err(|_| "bridge poisoned".to_string())?
        .clone();
    tx.send(Call {
        tool: tool.to_string(),
        args,
        reply,
    })
    .map_err(|_| "the app is no longer running".to_string())?;

    // Nudge the UI thread: without this an idle window would not repaint and
    // the call would sit in the queue until the user moved the mouse.
    if let Some(slot) = WAKE.get() {
        if let Ok(guard) = slot.lock() {
            if let Some(ctx) = guard.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    answer
        .recv()
        .map_err(|_| "the app closed before answering".to_string())?
}
