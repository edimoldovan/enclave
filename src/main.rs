use std::path::PathBuf;

use eframe::egui;
use enclave::role::{self, Role};
use enclave::{host, mcp, posthost};
use grido::state::GridoApp;
use post_ui::PostApp;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match role::of(&args) {
        // The stdio shim an MCP client launches. It answers the handshake and
        // the read-only tools itself and opens nothing; anything that acts goes
        // to the server, which is the only place the user can be asked first.
        Role::Shim => {
            if let Err(e) = mcp::proto::stdio_shim() {
                eprintln!("enclave mcp: {e}");
                std::process::exit(1);
            }
            Ok(())
        }

        // Wires this install into the MCP clients it finds.
        Role::Register => {
            let (done, skipped) = grido::mcp::register::register_all();
            if done.is_empty() {
                println!("Registered with nothing.");
            } else {
                println!("Registered with: {}", done.join(", "));
            }
            for note in skipped {
                println!("Skipped {note}");
            }
            Ok(())
        }

        // `enclave post <verb>`: the terminal door onto the mail palette. The
        // same library calls the assistant's post_ tools make, and free like
        // them — nothing in v1 prompts.
        Role::Post { words } => {
            match post::verbs::cli(&words) {
                Ok(text) => {
                    if !text.is_empty() {
                        println!("{text}");
                    }
                }
                Err(e) => {
                    eprintln!("enclave post: {e}");
                    std::process::exit(1);
                }
            }
            Ok(())
        }

        // The mail verbs that are views: `enclave post`, `post list <email>`,
        // `post read <email> <id>`. Reading is what a window is for, so these
        // put one on screen instead of printing — or hand the view to the one
        // already up. Either way the prompt comes straight back: the window is
        // a detached process of its own, never this one.
        Role::PostOpen { view } => {
            if let Err(e) = posthost::open(&view) {
                eprintln!("enclave post: {e}");
                std::process::exit(1);
            }
            Ok(())
        }

        // And that detached process, reading its own role back.
        Role::PostWindow { view } => post_window(view),

        // The server: the socket, the registry, the confirmation broker. No
        // window of its own, so it needs no display to run.
        Role::Serve => {
            enclave::serve::run();
            Ok(())
        }

        // One confirmation dialog, for the question on stdin.
        Role::Confirm => enclave::dialog::run(),

        Role::Window { product, path } => window(product, path),
    }
}

/// Shows Post at one view — or finds that a Post window already is, and hands
/// the view over instead of opening a second one.
///
/// Two things happen before the event loop, and both are the HTML body pane's:
/// GTK has to be started on this thread for WebKit, and the window has to ask
/// for X11, because a child webview is impossible on Wayland. Neither is fatal
/// — without them the detail view shows the message's text, or opens the body
/// in a window of its own — so nothing here refuses to run.
fn post_window(view: post_ui::View) -> eframe::Result {
    let ready = post_ui::web::prepare();

    let open_rx = match posthost::start(&view) {
        posthost::Outcome::HandedOff => return Ok(()),
        posthost::Outcome::Run(rx) => rx,
    };

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_title("Post")
            // Wayland's half of the same name `posthost` spawns the window
            // under; X11 takes it from argv[0] instead.
            .with_app_id(posthost::APP_ID),
        ..Default::default()
    };
    if ready.x11 {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
    }

    let outcome = eframe::run_native(
        "enclave-post",
        options,
        Box::new(move |cc| {
            // GTK is in this process too, and its X errors would otherwise be
            // filed against winit's connection and raised at the next focus
            // change. Done here because this is where winit's own connection
            // can be named, and before any webview exists to make one.
            enclave::xerror::quarantine_foreign(winit_display(cc));
            enclave_ui::fonts::install(&cc.egui_ctx);
            cc.egui_ctx
                .set_visuals(enclave_ui::theme::visuals(&enclave_ui::theme::load()));
            posthost::set_wake(cc.egui_ctx.clone());
            let mut app = PostApp::new(view).with_ready(ready);
            app.wake = Some(cc.egui_ctx.clone());
            app.open_rx = Some(open_rx);
            Ok(Box::new(app))
        }),
    );
    // The window is gone: so is the door, or the next launch would talk to it.
    posthost::release();
    outcome
}

/// winit's own X connection as a number, or zero when this window is not on
/// X11 at all — on Wayland there is no Xlib error handler to share.
fn winit_display(cc: &eframe::CreationContext<'_>) -> usize {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    match cc.display_handle().map(|handle| handle.as_raw()) {
        Ok(RawDisplayHandle::Xlib(x11)) => x11.display.map_or(0, |display| display.as_ptr() as usize),
        _ => 0,
    }
}

/// Shows a product — or finds that another window already is, and hands its
/// file over instead of opening a second one.
fn window(product: String, path: Option<PathBuf>) -> eframe::Result {
    if product != role::DEFAULT_PRODUCT {
        eprintln!("enclave: there is no product called \"{product}\"");
        std::process::exit(2);
    }
    let link = match host::start(&product, path.as_deref()) {
        host::Outcome::HandedOff => return Ok(()),
        host::Outcome::Run(link) => link,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Grido"),
        ..Default::default()
    };
    eframe::run_native(
        "enclave",
        options,
        Box::new(move |cc| {
            enclave_ui::fonts::install(&cc.egui_ctx);
            // Wear the Omarchy theme from the first frame.
            cc.egui_ctx
                .set_visuals(enclave_ui::theme::visuals(&enclave_ui::theme::load()));
            let mut app = GridoApp::new(path);

            // The bridge carries a tool call from the socket thread onto this
            // one, where the workbook lives. It must exist before the server is
            // told we are here, so start it first.
            let rx = mcp::bridge::install();
            mcp::bridge::set_wake(cc.egui_ctx.clone());
            app.mcp_rx = Some(rx);
            app.mcp_socket = Some(mcp::proto::socket_path());
            app.mcp_serving = link.is_some();
            if let Some(link) = link {
                host::serve(product, link);
            }

            Ok(Box::new(app))
        }),
    )
}
