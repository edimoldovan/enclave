//! Whose X error is it?
//!
//! Xlib's error handler is one per *process*, not one per connection, and winit
//! installs its own when it opens the display. Post also runs GTK in that same
//! process — the HTML body pane is a WebKit view — and GTK has a display
//! connection of its own. So from the moment the event loop starts, every X
//! error GTK, GDK and WebKit make is delivered to winit's handler, which files
//! it as the latest error on *its* connection.
//!
//! winit then reads that file on the next round trip it makes, and the first
//! one after a focus change is the input context's:
//!
//! ```text
//! ime.borrow_mut().unfocus(…).expect("Failed to unfocus input context")
//! ```
//!
//! — so a GLX error WebKit made while tearing down a message body ends the mail
//! window, at the next key press that moves focus. Disabling IME does not close
//! this: winit gives every window an input context at creation whether IME is
//! allowed or not, and `unfocus` runs on it either way.
//!
//! The fix is to put the errors back where they belong. winit takes hooks, and
//! documents that a hook returning true keeps the error from winit; this
//! registers one that claims every error that did not arrive on winit's own
//! connection. Errors winit made are still winit's, and behave exactly as
//! before.

/// Tells winit to ignore X errors from every other connection in this process.
///
/// `ours` is winit's own `Display *` as a number, from the window's display
/// handle. Zero means we could not find out, and then nothing is claimed.
pub fn quarantine_foreign(ours: usize) {
    if ours == 0 {
        return;
    }
    winit::platform::x11::register_xlib_error_hook(Box::new(move |display, _event| {
        foreign(display as usize, ours)
    }));
}

/// True when an error arrived on a connection that is not winit's — GTK's, in
/// this process — and so is not winit's to panic about.
pub fn foreign(display: usize, ours: usize) -> bool {
    ours != 0 && display != ours
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the other connections are claimed. winit's own errors are left to
    /// winit, and so is everything if we never learned which connection is its.
    #[test]
    fn an_error_belongs_to_the_connection_it_arrived_on() {
        let winit = 0x7f00_1000;
        let gtk = 0x7f00_2000;
        assert!(foreign(gtk, winit), "GTK's error is not winit's to raise");
        assert!(!foreign(winit, winit), "winit still hears itself");
        assert!(!foreign(gtk, 0), "with no connection to compare, claim nothing");
    }
}
