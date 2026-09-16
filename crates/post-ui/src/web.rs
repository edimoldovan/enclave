//! The HTML body pane, and the platform truth behind it.
//!
//! Real mail is HTML, so Post renders the HTML the mail library already saved
//! rather than a flattened copy of it. The renderer is a WebKit view (`wry`),
//! and where it can live is decided by the display server:
//!
//! - **X11** — the view is a child window laid over the detail pane's rectangle,
//!   inside the Post window. This is the one that looks like one app.
//! - **Wayland** — a child view is impossible: `wry` says so plainly, and
//!   `build_as_child` refuses. So this window asks for X11 at startup, and if
//!   there is no X display to ask for, the body opens in a window of its own
//!   beside the Post window. Under a tiling compositor that is a pane.
//! - **Neither** — the detail view shows the message's text, which the mail
//!   library extracts for the assistant anyway. Nothing is lost but the layout.
//!
//! GTK is what WebKit needs, and it has to be started on this thread before the
//! event loop and stepped alongside it — [`prepare`] and [`pump`] are those two
//! halves.
//!
//! One view serves the whole window: starting WebKit costs a noticeable moment
//! and pointing it at another document costs nothing, so the pane is built on
//! the first message opened, kept for the window's life, and hidden when there
//! is no message to show. The keyboard follows from that — while the view has
//! the focus the window behind it sees no keys at all, so the chords the
//! window claims are taken from GTK and handed up; see [`chord`].
//!
//! Built without the `webview` feature (a computer with no webkit2gtk-4.1),
//! every function here is still present and answers "nowhere", so the detail
//! view compiles and runs with the text body.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// Where a message's body ended up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Placed {
    /// No webview: the detail pane draws the text body itself.
    #[default]
    Nowhere,
    /// A child view over the detail pane — one window, as intended.
    Pane,
    /// A window of its own beside this one, because the pane was refused.
    Beside,
}

/// A rectangle in physical pixels, which is what a webview is placed in.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Px {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What starting up found: whether GTK came up, and whether this window can ask
/// for X11 (and so for a body pane inside itself).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Ready {
    pub gtk: bool,
    pub x11: bool,
}

impl Ready {
    /// One line for the status bar when the body cannot be a pane.
    pub fn note(&self) -> Option<&'static str> {
        match (self.gtk, self.x11) {
            (true, true) => None,
            (true, false) => Some("No X display: HTML bodies open in a window of their own."),
            (false, _) => Some("No webview on this computer: messages show as text."),
        }
    }
}

/// How much saved HTML is worth handing to WebKit. A mail body past this is a
/// mailing list that inlined its own images; the text body is the better read.
const MAX_HTML: usize = 4 * 1024 * 1024;

/// The message as a page, with the rules it is rendered under written into it.
///
/// A message is a document to look at, never a program to run. Its pictures
/// load — the parts it carries itself as `data:` or `cid:`, and the ones it
/// points at over the network, which is most of real mail — and nothing else
/// does: no scripts, no frames, no fonts, no forms, no call back to anywhere.
/// That holds whatever the message's own markup asks for, because policies
/// from several `<meta>` tags combine into the strictest of them rather than
/// the last one.
pub fn page(html: &str, bg: [u8; 3], fg: [u8; 3], link: [u8; 3]) -> String {
    let body = if html.len() > MAX_HTML {
        &html[..floor_char_boundary(html, MAX_HTML)]
    } else {
        html
    };
    let (before, after) = split_doctype(body);
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         img-src https: http: data: cid:; style-src 'unsafe-inline'; font-src data:; \
         script-src 'none'; frame-src 'none'; media-src 'none'; object-src 'none'; \
         connect-src 'none'; form-action 'none'; base-uri 'none'\">\
         <meta name=\"referrer\" content=\"no-referrer\">\
         <style>html,body{{margin:0;padding:0;background:{bg};color:{fg};\
         font:14px/1.5 system-ui,sans-serif;overflow-wrap:break-word}}\
         body{{padding:14px}}\
         a{{color:{link}}} img{{max-width:100%;height:auto}} \
         table{{max-width:100%}}</style></head><body>{before}{after}</body></html>",
        bg = hex(bg),
        fg = hex(fg),
        link = hex(link),
    )
}

/// The message either side of the `<!doctype>` it came with, so the page
/// around it declares the only one.
///
/// A saved message is usually a whole document, and a doctype is a thing a
/// document may state once, at the top. Nested in this page it is a token the
/// parser drops; it is dropped here instead, where it can be seen. Mail often
/// puts a comment before it, so the comments are stepped over to find it.
fn split_doctype(html: &str) -> (&str, &str) {
    let mut at = 0;
    loop {
        let rest = html[at..].trim_start();
        at = html.len() - rest.len();
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                // Past this comment, and always forward: this cannot spin.
                Some(end) => at += end + "-->".len(),
                None => return (html, ""),
            }
            continue;
        }
        let doctype = rest
            .as_bytes()
            .get(.."<!doctype".len())
            .is_some_and(|start| start.eq_ignore_ascii_case(b"<!doctype"));
        return match doctype.then(|| rest.find('>')).flatten() {
            Some(end) => (&html[..at], &html[at + end + 1..]),
            None => (html, ""),
        };
    }
}

/// The largest index at or below `at` that does not split a character.
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The navigation rule a message is read under: the document Post asked for
/// loads, and nothing that document points at ever does.
///
/// WebKit asks about the message's own load first — `load_html` is a
/// navigation to `about:blank` like any other — so a handler that refuses
/// every navigation refuses the message itself, and the view keeps the blank
/// white it was built as. One pane reads message after message, so the
/// permission belongs to the load rather than to the view: each deliberate
/// load arms exactly one navigation. That one is the message's and is yes; a
/// clicked link, a `meta refresh`, anything after it, is no.
#[derive(Debug)]
pub struct OnlyTheDocument(std::cell::Cell<bool>);

impl Default for OnlyTheDocument {
    /// Armed: a view is built with its first message already in it.
    fn default() -> Self {
        Self(std::cell::Cell::new(true))
    }
}

impl OnlyTheDocument {
    /// Lets the next navigation through — said by the load that expects it.
    pub fn arm(&self) {
        self.0.set(true);
    }

    /// True for the load that was armed, false every time after it.
    pub fn allows(&self) -> bool {
        self.0.replace(false)
    }
}

/// The chord a key pressed over the body pane is, in the terms the keymap is
/// written in — or `None` for a key no binding can name, which is WebKit's.
///
/// GTK hands over an X11 keysym and a modifier mask; a binding is an egui
/// shortcut. This is the one translation between the two, so what the pane
/// hands back is what the keymap says rather than a second list of keys.
pub fn chord(keysym: u32, ctrl: bool, alt: bool, shift: bool) -> Option<KeyboardShortcut> {
    let name = match keysym {
        0xff1b => "Escape",
        0xff08 => "Backspace",
        0xff09 | 0xfe20 => "Tab",
        0xff0d | 0xff8d => "Enter",
        0xff50 => "Home",
        0xff57 => "End",
        0xff55 => "PageUp",
        0xff56 => "PageDown",
        0xff63 => "Insert",
        0xffff => "Delete",
        0xff51 => "ArrowLeft",
        0xff52 => "ArrowUp",
        0xff53 => "ArrowRight",
        0xff54 => "ArrowDown",
        0x0020 => "Space",
        f @ 0xffbe..=0xffc9 => {
            return chord_named(&format!("F{}", f - 0xffbe + 1), ctrl, alt, shift);
        }
        // A letter or a digit is its own character, and the keymap spells it
        // that way too.
        c @ (0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a) => {
            return chord_named(&(c as u8 as char).to_string(), ctrl, alt, shift);
        }
        _ => return None,
    };
    chord_named(name, ctrl, alt, shift)
}

/// The same, once the key has a name egui knows. Built the way the keymap
/// loader builds one, so the two compare equal.
fn chord_named(name: &str, ctrl: bool, alt: bool, shift: bool) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    modifiers.ctrl = ctrl;
    modifiers.alt = alt;
    modifiers.shift = shift;
    Some(KeyboardShortcut::new(modifiers, Key::from_name(name)?))
}

// --------------------------------------------------------------- with a webview

#[cfg(feature = "webview")]
mod real {
    use std::rc::Rc;
    use std::sync::mpsc::{Receiver, Sender};

    use super::{KeyboardShortcut, OnlyTheDocument, Placed, Px, Ready};

    use wry::dpi::{PhysicalPosition, PhysicalSize};
    use wry::{NewWindowResponse, Rect, WebView, WebViewBuilder};

    /// Starts GTK, asking it for X11 first so a body pane is possible.
    ///
    /// `GDK_BACKEND` is the only way to tell GDK which display server to open,
    /// and it is read once, here. It is put back immediately afterwards so
    /// nothing this window later starts — the browser the add-account flow
    /// opens, above all — inherits a forced backend.
    ///
    /// # Safety
    ///
    /// Called on the main thread before the event loop and before any thread of
    /// this process exists, which is what makes writing the environment sound.
    pub fn prepare() -> Ready {
        let x11_display = std::env::var("DISPLAY").is_ok_and(|d| !d.is_empty());
        if !x11_display {
            return Ready {
                gtk: start_gtk(),
                x11: false,
            };
        }
        let restore = std::env::var_os("GDK_BACKEND");
        // SAFETY: main thread, before any other thread is started.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
        let gtk = start_gtk();
        // SAFETY: same thread, still before any other thread is started.
        unsafe {
            match restore {
                Some(had) => std::env::set_var("GDK_BACKEND", had),
                None => std::env::remove_var("GDK_BACKEND"),
            }
        }
        if gtk {
            return Ready { gtk, x11: true };
        }
        // X11 was there but GTK would not take it. Let GTK choose for itself;
        // the body then opens in its own window.
        Ready {
            gtk: start_gtk(),
            x11: false,
        }
    }

    #[cfg(target_os = "linux")]
    fn start_gtk() -> bool {
        gtk::init().is_ok()
    }

    #[cfg(not(target_os = "linux"))]
    fn start_gtk() -> bool {
        true
    }

    /// Steps GTK's own loop. WebKit does its work here, so this runs every
    /// frame the Post window draws.
    #[cfg(target_os = "linux")]
    pub fn pump() {
        // Bounded: a page that keeps producing events must not starve the frame.
        for _ in 0..64 {
            if !gtk::events_pending() {
                return;
            }
            gtk::main_iteration_do(false);
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn pump() {}

    /// The window's one webview, and the window it needed when it could not be
    /// a pane. Built on the first message and kept: what changes per message is
    /// the document loaded into it.
    pub struct Body {
        view: Option<WebView>,
        #[cfg(target_os = "linux")]
        window: Option<gtk::Window>,
        /// Where the view lives, once there is one. Unlike `placed`, this
        /// outlasts the message on screen.
        kind: Placed,
        /// Set by the one attempt to build a view, so a computer that refuses
        /// one is not asked again on every message.
        tried: bool,
        placed: Placed,
        /// Which message is in it, so a redraw does not reload it.
        showing: Option<String>,
        /// The placement last handed to the platform: the rectangle in whole
        /// physical pixels, and the density it was converted at. Nothing but a
        /// skip of the identical call rests on this: it is dropped whenever the
        /// view is shown, hidden or loaded, so the next frame places the pane
        /// again from scratch.
        applied: Option<(Exact, f64)>,
        /// What `set_visible` was last told, so it is told once.
        visible: bool,
        /// Armed by each load, spent by the navigation it allows.
        rule: Rc<OnlyTheDocument>,
        /// The chords the pane hands back instead of keeping.
        forward: Vec<KeyboardShortcut>,
        keys_tx: Sender<KeyboardShortcut>,
        keys_rx: Receiver<KeyboardShortcut>,
    }

    impl Default for Body {
        fn default() -> Self {
            Self::new(Vec::new())
        }
    }

    impl Body {
        /// A pane that hands `forward` back to the window and leaves every
        /// other key to WebKit.
        pub fn new(forward: Vec<KeyboardShortcut>) -> Body {
            let (keys_tx, keys_rx) = std::sync::mpsc::channel();
            Body {
                view: None,
                #[cfg(target_os = "linux")]
                window: None,
                kind: Placed::Nowhere,
                tried: false,
                placed: Placed::Nowhere,
                showing: None,
                applied: None,
                visible: false,
                rule: Rc::new(OnlyTheDocument::default()),
                forward,
                keys_tx,
                keys_rx,
            }
        }

        pub fn placed(&self) -> Placed {
            self.placed
        }

        /// True when this body is already on screen.
        pub fn showing(&self, key: &str) -> bool {
            self.showing.as_deref() == Some(key)
        }

        /// How many of the page's own pixels go in one point of this window,
        /// where `ppp` is what egui draws a point as.
        ///
        /// The pane is placed in physical pixels and WebKit draws a CSS pixel
        /// in one of those, unless GDK is scaling the native side too. So a
        /// page that says 14px text is saying 14 of these, and a window whose
        /// point is more than one pixel is showing that page smaller than the
        /// same numbers would be here. Anything measuring the page in this
        /// window's units divides by this.
        pub fn density(&self, ppp: f32) -> f32 {
            let scale = self.view.as_ref().map(scale_of).unwrap_or(1.0) as f32;
            (ppp / scale.max(1.0)).max(0.1)
        }

        /// The chords pressed over the pane since this was last asked.
        pub fn pressed(&mut self) -> Vec<KeyboardShortcut> {
            self.keys_rx.try_iter().collect()
        }

        /// Puts `html` on screen for message `key`, over `at`. Returns where it
        /// ended up; `Placed::Nowhere` means the caller draws the text instead.
        pub fn show(
            &mut self,
            parent: &eframe::Frame,
            ready: Ready,
            key: &str,
            html: &str,
            at: Px,
        ) -> Placed {
            if !self.showing(key) {
                // Remembered whatever happens next, so a body that cannot be
                // built is not attempted again on every frame.
                self.showing = Some(key.to_string());
                self.load(parent, ready, html, at);
                self.placed = self.kind;
            }
            // On screen first and placed second: showing a view hands its
            // window back to GTK, which sizes it from its own idea of how big
            // it should be, so the bounds have to be the last word.
            self.set_visible(true);
            self.place(at);
            self.placed
        }

        /// One message into the view, building the view the first time. WebKit
        /// is expensive to start and cheap to point at another document, so it
        /// is started once.
        fn load(&mut self, parent: &eframe::Frame, ready: Ready, html: &str, at: Px) {
            // A new document is a new chance for WebKit to size the view from
            // its own idea of the page: whatever the pane was last placed at
            // counts for nothing now.
            self.applied = None;
            if let Some(view) = &self.view {
                self.rule.arm();
                let _ = view.load_html(html);
                return;
            }
            if self.tried || !ready.gtk {
                return;
            }
            self.tried = true;
            let child = ready
                .x11
                .then(|| child(parent, html, at, self.rule.clone()))
                .flatten();
            match child {
                Some(view) => {
                    self.view = Some(view);
                    self.kind = Placed::Pane;
                }
                None => {
                    if let Some(view) = beside(html, self.rule.clone()) {
                        self.view = Some(view.0);
                        #[cfg(target_os = "linux")]
                        {
                            self.window = Some(view.1);
                        }
                        self.kind = Placed::Beside;
                    }
                }
            }
            #[cfg(target_os = "linux")]
            if let Some(view) = &self.view {
                watch_keys(view, self.forward.clone(), self.keys_tx.clone());
            }
        }

        /// Shows or hides the pane without tearing it down.
        ///
        /// A child view is a native window over the app's own drawing, so
        /// nothing egui puts on top of it — the shortcut viewer above all —
        /// would be visible while it is up. It steps aside instead.
        pub fn set_shown(&mut self, shown: bool) {
            if self.kind == Placed::Pane {
                self.set_visible(shown);
            }
        }

        /// Moves the pane to `at`, which is the rectangle the detail view
        /// measured this frame.
        ///
        /// The rectangle the detail view measured is the truth, every frame:
        /// anything other than the exact rectangle already applied is applied
        /// again, and applied all the way down to the native window rather
        /// than left to whatever GTK would have allocated by itself. The one
        /// thing skipped is the call that would change nothing — which is the
        /// same rectangle *at the same density*: the native side is told the
        /// rectangle divided by GDK's scale, so the same rectangle on a screen
        /// that changed density is a different window, and is placed again.
        pub fn place(&mut self, at: Px) {
            let Some(view) = &self.view else {
                return;
            };
            let scale = scale_of(view);
            let want = placement(at, scale);
            if self.applied == Some(want) {
                return;
            }
            let whole = lands_in(at, scale);
            match self.kind {
                Placed::Pane => {
                    let _ = view.set_bounds(bounds(at));
                    #[cfg(target_os = "linux")]
                    force(view, whole);
                }
                // A body in a window of its own is the compositor's to put
                // somewhere and this window's to size: it takes the pane's
                // height and keeps the width it was given. Under a tiling
                // compositor the request is ignored and it stays tiled, which
                // is the right answer there.
                Placed::Beside => {
                    #[cfg(target_os = "linux")]
                    if let Some(window) = &self.window {
                        use gtk::prelude::GtkWindowExt;
                        let (wide, _) = window.size();
                        window.resize(wide.max(1), whole.h.max(1));
                    }
                }
                Placed::Nowhere => return,
            }
            self.applied = Some(want);
        }

        /// TEMPORARY probe: the X server's own geometry for the child window.
        #[doc(hidden)]
        pub fn debug_geometry(&self) -> Option<(i32, i32, i32, i32)> {
            let view = self.view.as_ref()?;
            let b = view.bounds().ok()?;
            let (x, y) = b.position.to_logical::<i32>(1.0).into();
            let (w, h) = b.size.to_logical::<i32>(1.0).into();
            Some((x, y, w, h))
        }

        /// TEMPORARY probe: what was last applied.
        #[doc(hidden)]
        pub fn debug_applied(&self) -> Option<(i32, i32, i32, i32)> {
            self.applied.map(|(e, _)| (e.x, e.y, e.w, e.h))
        }

        /// TEMPORARY probe: the scale the gate converts with.
        #[doc(hidden)]
        pub fn debug_scale(&self) -> f64 {
            self.view.as_ref().map(scale_of).unwrap_or(0.0)
        }

        /// Takes the body off screen — leaving the detail view, or opening a
        /// different message. The view itself stays, ready for the next one.
        pub fn hide(&mut self) {
            if self.showing.is_none() && !self.visible {
                return;
            }
            self.showing = None;
            self.placed = Placed::Nowhere;
            self.applied = None;
            self.set_visible(false);
        }

        /// The view on screen or off it, said once per change.
        fn set_visible(&mut self, shown: bool) {
            if self.visible == shown || self.view.is_none() {
                return;
            }
            self.visible = shown;
            // Showing a view hands its window back to GTK, which sizes it from
            // its own idea of how big it should be. Whatever it was placed at
            // before is no longer where it is: place it again next frame.
            self.applied = None;
            if let Some(view) = &self.view {
                let _ = view.set_visible(shown);
            }
            #[cfg(target_os = "linux")]
            if let Some(window) = &self.window {
                use gtk::prelude::WidgetExt;
                if shown {
                    window.show_all();
                } else {
                    window.hide();
                }
            }
        }
    }

    fn bounds(at: Px) -> Rect {
        Rect {
            position: PhysicalPosition::new(at.x, at.y).into(),
            size: PhysicalSize::new(at.w.max(1.0), at.h.max(1.0)).into(),
        }
    }

    /// Where a pane lands: the rectangle in the whole pixels the platform
    /// works in, which is what `set_bounds` turns its argument into.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Whole {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    fn lands_in(at: Px, scale: f64) -> Whole {
        let Rect { position, size } = bounds(at);
        let (x, y) = position.to_logical::<i32>(scale).into();
        let (w, h) = size.to_logical::<i32>(scale).into();
        Whole { x, y, w, h }
    }

    /// The rectangle asked for, in whole physical pixels — the thing compared
    /// against the last one applied, and nothing more than that.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Exact {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    fn exact(at: Px) -> Exact {
        Exact {
            x: at.x.round() as i32,
            y: at.y.round() as i32,
            w: at.w.max(1.0).round() as i32,
            h: at.h.max(1.0).round() as i32,
        }
    }

    /// What a placement is: the rectangle asked for and the density it was
    /// placed at. Both, because the native side is told the one divided by the
    /// other — the same rectangle at a new density lands somewhere else.
    fn placement(at: Px, scale: f64) -> (Exact, f64) {
        (exact(at), scale)
    }

    /// The same rectangle again, straight at the native window under the view.
    ///
    /// `set_bounds` allocates the view's window and leaves GTK to agree. GTK
    /// does not always agree: the window it was given keeps the size it was
    /// first allocated, so a pane built at one size stays that size however
    /// the window around it grows. So the size is also *requested* — which is
    /// the number GTK's own layout uses — and the X window is moved and
    /// resized itself, which nothing downstream re-decides.
    #[cfg(target_os = "linux")]
    fn force(view: &WebView, whole: Whole) {
        use gtk::prelude::{Cast, GtkWindowExt, WidgetExt};
        use wry::WebViewExtUnix;

        let (w, h) = (whole.w.max(1), whole.h.max(1));
        let widget: gtk::Widget = view.webview().upcast();
        widget.set_size_request(w, h);
        widget.queue_resize();

        let Some(top) = widget.toplevel() else {
            return;
        };
        if let Ok(window) = top.clone().downcast::<gtk::Window>() {
            window.set_size_request(w, h);
            window.resize(w, h);
            window.move_(whole.x, whole.y);
        }
        // The view lives in a window of the app's own window: where that sits
        // is an X rectangle, and this sets it.
        if let Some(native) = top.window() {
            native.move_resize(whole.x, whole.y, w, h);
        }
        top.queue_resize();
    }

    /// The scale the view is placed at, which is GDK's rather than egui's: the
    /// two are separate numbers and need not agree.
    #[cfg(target_os = "linux")]
    fn scale_of(view: &WebView) -> f64 {
        use gtk::prelude::WidgetExt;
        use wry::WebViewExtUnix;

        // Never zero: dpi refuses to convert at a scale of nothing.
        view.webview().scale_factor().max(1) as f64
    }

    #[cfg(not(target_os = "linux"))]
    fn scale_of(_view: &WebView) -> f64 {
        1.0
    }

    /// The shared rules: a message is a document to look at, never a program to
    /// run and never a page to navigate.
    fn locked_down(html: &str, rule: Rc<OnlyTheDocument>) -> WebViewBuilder<'_> {
        WebViewBuilder::new()
            .with_html(html)
            .with_javascript_disabled()
            .with_transparent(false)
            .with_back_forward_navigation_gestures(false)
            // A link in a message goes nowhere from here: no navigation past
            // the load this view was asked for, and no window opened on its
            // behalf.
            .with_navigation_handler(move |_url| rule.allows())
            .with_new_window_req_handler(|_url, _features| NewWindowResponse::Deny)
    }

    /// Hands the window back the keys it has bindings for, and leaves WebKit
    /// every other one.
    ///
    /// A body pane is a native view with the keyboard to itself: while it has
    /// the focus the window behind it is sent nothing, and Escape over a
    /// message would be dead. GTK sees a key before WebKit does — this signal
    /// runs ahead of the window's own handling, which is what passes it on to
    /// the page — so a chord the window has claimed is sent up and stopped
    /// here, and everything else falls through untouched: scrolling a body with
    /// the arrows is WebKit's to do.
    #[cfg(target_os = "linux")]
    fn watch_keys(
        view: &WebView,
        forward: Vec<KeyboardShortcut>,
        keys: Sender<KeyboardShortcut>,
    ) {
        use gtk::glib::Propagation;
        use gtk::prelude::{Cast, WidgetExt};
        use wry::WebViewExtUnix;

        let widget: gtk::Widget = view.webview().upcast();
        // Keys arrive at the window the view is in and are handed down from
        // there, so that is where they can still be taken.
        let target = widget.toplevel().unwrap_or(widget);
        target.connect_key_press_event(move |_widget, event| {
            let state = event.state();
            let pressed = super::chord(
                *event.keyval(),
                state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
                state.contains(gtk::gdk::ModifierType::MOD1_MASK),
                state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            );
            match pressed {
                Some(chord) if forward.contains(&chord) => {
                    let _ = keys.send(chord);
                    Propagation::Stop
                }
                _ => Propagation::Proceed,
            }
        });
    }

    /// The body as a child view over the detail pane. X11 only — on Wayland
    /// `wry` refuses, which is the whole reason the other path exists.
    fn child(
        parent: &eframe::Frame,
        html: &str,
        at: Px,
        rule: Rc<OnlyTheDocument>,
    ) -> Option<WebView> {
        locked_down(html, rule)
            .with_bounds(bounds(at))
            .build_as_child(parent)
            .ok()
    }

    /// The body in a window of its own, beside this one.
    #[cfg(target_os = "linux")]
    fn beside(html: &str, rule: Rc<OnlyTheDocument>) -> Option<(WebView, gtk::Window)> {
        use gtk::prelude::{GtkWindowExt, WidgetExt};
        use wry::WebViewBuilderExtUnix;

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Post — message");
        window.set_default_size(720, 800);
        let view = locked_down(html, rule).build_gtk(&window).ok()?;
        // After the webview is in it, so the window comes up with the body
        // already there rather than empty for a frame.
        window.show_all();
        Some((view, window))
    }

    /// Where a child view always works, there is nothing to fall back to.
    #[cfg(not(target_os = "linux"))]
    fn beside(_html: &str, _rule: Rc<OnlyTheDocument>) -> Option<(WebView, ())> {
        None
    }

    #[cfg(test)]
    mod tests {
        use super::{exact, lands_in, placement, Body, Px, Whole};

        /// What the pane is compared by: the rectangle asked for, in whole
        /// physical pixels. The same rectangle frame after frame is the same
        /// rectangle — that one call is skipped — and a window that grew,
        /// shrank or moved by a pixel is not.
        #[test]
        fn the_same_rectangle_is_the_same_and_anything_else_is_not() {
            let pane = Px {
                x: 11.0,
                y: 103.0,
                w: 1535.0,
                h: 963.0,
            };
            let first = exact(pane);
            assert_eq!(exact(pane), first, "the next frame, and every one after");

            // The window grew, shrank, and moved.
            assert_ne!(exact(Px { w: 1872.0, ..pane }), first);
            assert_ne!(exact(Px { h: 700.0, ..pane }), first);
            assert_ne!(exact(Px { y: 140.0, ..pane }), first);
            assert_ne!(exact(Px { w: 1536.0, ..pane }), first);

            // A rectangle of nothing is still a window: one pixel, never zero.
            let none = exact(Px {
                w: 0.0,
                h: 0.0,
                ..pane
            });
            assert_eq!((none.w, none.h), (1, 1));
        }

        /// The rectangle is compared in whole physical pixels, so a fraction of
        /// one is the same rectangle and the call is skipped; a whole one is a
        /// different rectangle and the pane is placed again.
        #[test]
        fn a_fraction_of_a_pixel_is_still_the_same_rectangle() {
            let pane = Px {
                x: 11.0,
                y: 103.0,
                w: 1535.0,
                h: 963.0,
            };
            let first = exact(pane);
            assert_eq!(exact(Px { w: 1535.4, ..pane }), first);
            assert_eq!(exact(Px { x: 10.6, ..pane }), first);
            assert_eq!(exact(Px { y: 103.4, ..pane }), first);
            assert_ne!(exact(Px { w: 1535.6, ..pane }), first);
            assert_ne!(exact(Px { x: 11.6, ..pane }), first);

            // A rectangle of less than nothing is still a window.
            let inverted = exact(Px {
                w: -40.0,
                h: -1.0,
                ..pane
            });
            assert_eq!((inverted.w, inverted.h), (1, 1));
        }

        /// Nothing is remembered until there is a view to place. A body built
        /// on a computer that gave it no webview applies nothing, keeps
        /// nothing, and hiding it is not an error.
        #[test]
        fn a_body_with_no_view_remembers_no_rectangle() {
            let mut body = Body::default();
            assert_eq!(body.debug_applied(), None);

            body.place(Px {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            });
            assert_eq!(body.debug_applied(), None, "there was nothing to place");

            body.hide();
            assert_eq!(body.debug_applied(), None);
            assert_eq!(body.debug_geometry(), None);
            assert_eq!(body.debug_scale(), 0.0);
        }

        /// Where that rectangle lands, which is what the native side is told:
        /// the physical rectangle divided by the screen's density, exactly as
        /// `set_bounds` divides it.
        #[test]
        fn a_pane_lands_where_the_platform_puts_it() {
            let pane = Px {
                x: 10.0,
                y: 100.0,
                w: 1536.0,
                h: 960.0,
            };
            assert_eq!(
                lands_in(pane, 1.0),
                Whole {
                    x: 10,
                    y: 100,
                    w: 1536,
                    h: 960
                }
            );
            assert_eq!(
                lands_in(pane, 2.0),
                Whole {
                    x: 5,
                    y: 50,
                    w: 768,
                    h: 480
                }
            );
            // The same rectangle at a different density is a different
            // rectangle on the native side.
            assert_ne!(lands_in(pane, 1.0), lands_in(pane, 2.0));
        }

        /// So the density is part of what a placement is compared by: a screen
        /// that changed density, with the pane's rectangle unmoved, is placed
        /// again rather than skipped as the call that would change nothing.
        #[test]
        fn the_same_rectangle_at_a_new_density_is_a_new_placement() {
            let pane = Px {
                x: 10.0,
                y: 100.0,
                w: 1536.0,
                h: 960.0,
            };
            let applied = placement(pane, 1.0);
            assert_eq!(
                placement(pane, 1.0),
                applied,
                "the next frame, and every one after"
            );
            assert_ne!(placement(pane, 2.0), applied, "a screen that changed density");
            assert_ne!(placement(Px { w: 800.0, ..pane }, 1.0), applied);

            // And the rectangle that would then be handed to the native side
            // is the one the new density asks for, not the one already there.
            assert_eq!(lands_in(pane, 2.0).w, 768);
        }
    }
}

// ------------------------------------------------------------ with no webview

#[cfg(not(feature = "webview"))]
mod real {
    use super::{KeyboardShortcut, Placed, Px, Ready};

    pub fn prepare() -> Ready {
        Ready::default()
    }

    pub fn pump() {}

    /// The same shape, answering "nowhere" — the detail view draws the text.
    #[derive(Default)]
    pub struct Body {
        showing: Option<String>,
    }

    impl Body {
        pub fn new(_forward: Vec<KeyboardShortcut>) -> Body {
            Body::default()
        }

        pub fn placed(&self) -> Placed {
            Placed::Nowhere
        }

        pub fn showing(&self, key: &str) -> bool {
            self.showing.as_deref() == Some(key)
        }

        /// No pane, so nothing is drawn in the page's pixels.
        pub fn density(&self, _ppp: f32) -> f32 {
            1.0
        }

        pub fn pressed(&mut self) -> Vec<KeyboardShortcut> {
            Vec::new()
        }

        pub fn show(
            &mut self,
            _parent: &eframe::Frame,
            _ready: Ready,
            _key: &str,
            _html: &str,
            _at: Px,
        ) -> Placed {
            Placed::Nowhere
        }

        pub fn set_shown(&mut self, _shown: bool) {}

        pub fn place(&mut self, _at: Px) {}

        pub fn hide(&mut self) {
            self.showing = None;
        }
    }
}

pub use real::{prepare, pump, Body};

#[cfg(test)]
mod tests {
    use super::*;

    const BG: [u8; 3] = [18, 20, 24];
    const FG: [u8; 3] = [200, 205, 212];
    const LINK: [u8; 3] = [95, 135, 190];

    /// The page a message is rendered as: its pictures load, wherever they
    /// live, and nothing else does — no script, no frame, no call out.
    #[test]
    fn a_message_is_rendered_as_pictures_and_nothing_else() {
        let page = page("<p>hello <img src=\"https://acme.test/x.gif\"></p>", BG, FG, LINK);
        assert!(page.contains("Content-Security-Policy"));
        assert!(page.contains("default-src 'none'"), "got {page}");
        // Images as the message has them: over the network, and its own parts.
        assert!(page.contains("img-src https: http: data: cid:"), "got {page}");
        assert!(page.contains("script-src 'none'"));
        assert!(page.contains("frame-src 'none'"));
        assert!(page.contains("object-src 'none'"));
        assert!(page.contains("connect-src 'none'"));
        assert!(page.contains("form-action 'none'"));
        assert!(page.contains("no-referrer"));
        // The message's own markup is still in there, untouched.
        assert!(page.contains("<p>hello <img src=\"https://acme.test/x.gif\"></p>"));
        // And it wears the app's colors rather than a white rectangle.
        assert!(page.contains("#121418"), "got {page}");
    }

    /// A saved message is a whole document — doctype, `<html>`, the lot — and
    /// the page it is put in stays one document all the same: one doctype, one
    /// body, the policy in it, and the message's own markup kept.
    #[test]
    fn a_whole_document_becomes_one_page() {
        for message in [
            "<!DOCTYPE html><html><body><p>a message</p></body></html>",
            "<!doctype html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \
             \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">\
             <html><body><p>a message</p></body></html>",
            // Mail puts a comment above the doctype often enough to matter.
            "<!-- saved --> \n<!doctype html><html><body><p>a message</p></body></html>",
            // And sometimes brings no doctype at all.
            "<html><body><p>a message</p></body></html>",
        ] {
            let page = page(message, BG, FG, LINK);
            assert_eq!(
                page.to_ascii_lowercase().matches("<!doctype").count(),
                1,
                "got {page}"
            );
            assert!(page.starts_with("<!doctype html><html><head>"), "got {page}");
            assert!(page.contains("Content-Security-Policy"), "got {page}");
            assert!(page.contains("<body>"), "got {page}");
            assert!(page.ends_with("</body></html>"), "got {page}");
            // Nothing of the message is lost along with its doctype.
            assert!(page.contains("<p>a message</p>"), "got {page}");
        }
    }

    /// The message's own load is the one navigation that happens; every
    /// navigation after it — a clicked link above all — is refused. Refusing
    /// that first one is refusing the message, and the pane stays blank.
    #[test]
    fn a_message_loads_itself_and_goes_nowhere_after() {
        let rule = OnlyTheDocument::default();
        assert!(rule.allows(), "the message's own document must load");
        assert!(!rule.allows());
        assert!(!rule.allows());
    }

    /// One pane, message after message: each load lets exactly one navigation
    /// through, and between two loads nothing may navigate at all — which is
    /// the clicked link in the message that is already up.
    #[test]
    fn every_load_allows_one_navigation_and_no_more() {
        let rule = OnlyTheDocument::default();
        assert!(rule.allows(), "the first message");
        assert!(!rule.allows(), "and nothing it points at");

        rule.arm();
        assert!(rule.allows(), "the next message");
        assert!(!rule.allows());
        assert!(!rule.allows());

        // Arming twice is still one navigation, not two.
        rule.arm();
        rule.arm();
        assert!(rule.allows());
        assert!(!rule.allows());
    }

    /// What the pane hands back to the window, and what it keeps. The chords
    /// come out exactly as the keymap parses them, because the window matches
    /// the two against each other.
    #[test]
    fn the_pane_gives_back_the_keys_the_keymap_names() {
        let of = |chord: &str| enclave_ui::keymap::parse_chord(chord).expect("a chord");

        // The back bindings, as GTK reports them: Escape, Backspace, Left,
        // and Alt+Left.
        assert_eq!(chord(0xff1b, false, false, false), Some(of("Escape")));
        assert_eq!(chord(0xff08, false, false, false), Some(of("Backspace")));
        assert_eq!(chord(0xff51, false, false, false), Some(of("ArrowLeft")));
        assert_eq!(chord(0xff51, false, true, false), Some(of("Alt+ArrowLeft")));
        // Any other binding a keymap may carry.
        assert_eq!(chord(0xffc2, false, false, false), Some(of("F5")));
        assert_eq!(chord(b'r' as u32, true, false, false), Some(of("Ctrl+R")));
        assert_eq!(chord(b'R' as u32, true, false, false), Some(of("Ctrl+R")));
        assert_eq!(chord(0xffff, false, false, false), Some(of("Delete")));
        assert_eq!(chord(b'7' as u32, false, false, false), Some(of("7")));

        // A modifier held is a different chord, so a binding without it is not
        // matched and the key stays WebKit's.
        assert_ne!(chord(0xff1b, true, false, false), Some(of("Escape")));
        // And a key no binding can name is never taken from the page.
        assert_eq!(chord(0xffe1, false, false, false), None, "Shift itself");
        assert_eq!(chord(0x0, false, false, false), None);
    }

    /// A body far past reading length is cut, and cut where a character ends —
    /// never mid-way through one.
    #[test]
    fn an_enormous_body_is_cut_cleanly() {
        let huge = "é".repeat(MAX_HTML);
        let page = page(&huge, BG, FG, LINK);
        assert!(page.len() < huge.len() + 2048);
        // Still a whole document, and still valid text.
        assert!(page.starts_with("<!doctype html>"));
        assert!(page.ends_with("</body></html>"));
    }

    /// What to say when the body cannot be a pane — one sentence per reason,
    /// and nothing at all when everything is where it should be.
    #[test]
    fn the_platform_says_what_it_could_not_do() {
        assert_eq!(
            Ready {
                gtk: true,
                x11: true
            }
            .note(),
            None
        );
        assert!(
            Ready {
                gtk: true,
                x11: false
            }
            .note()
            .expect("a sentence")
            .contains("window of their own")
        );
        assert!(
            Ready {
                gtk: false,
                x11: false
            }
            .note()
            .expect("a sentence")
            .contains("as text")
        );
    }

    /// A body that has not been shown is showing nothing, whichever way this
    /// was built.
    #[test]
    fn a_fresh_body_is_showing_nothing() {
        let body = Body::default();
        assert_eq!(body.placed(), Placed::Nowhere);
        assert!(!body.showing("18f3"));
    }
}
