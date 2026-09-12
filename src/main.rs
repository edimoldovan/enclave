use std::path::PathBuf;

use eframe::egui;
use enclave::role::{self, Role};
use enclave::{host, mcp};
use grido::state::GridoApp;

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

        // The server: the socket, the registry, the confirmation window.
        Role::Serve => enclave::serve::run(),

        Role::Window { product, path } => window(product, path),
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
