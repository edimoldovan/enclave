//! Which role a command line asks for.
//!
//! One executable, several jobs, and the arguments alone decide which — so the
//! binary an assistant launches, the server it starts, the dialog that server
//! puts on screen, and the window a person opens a file with are the same file
//! on disk.

use std::path::PathBuf;

/// The product a bare `enclave file.xlsx` means.
pub const DEFAULT_PRODUCT: &str = "grido";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// `enclave mcp` — speak MCP on stdin/stdout.
    Shim,
    /// `enclave register` — wire this install into the assistants found here.
    Register,
    /// `enclave --serve` — be the server. Headless.
    Serve,
    /// `enclave --confirm` — be one confirmation dialog: the question on stdin,
    /// the answer on stdout.
    Confirm,
    /// Anything else — show a product, or hand the file to the window that is
    /// already showing it.
    Window {
        product: String,
        path: Option<PathBuf>,
    },
}

/// Reads the arguments after the program name.
pub fn of<S: AsRef<str>>(args: &[S]) -> Role {
    match args.first().map(AsRef::as_ref) {
        Some("mcp") => return Role::Shim,
        Some("register") => return Role::Register,
        _ => {}
    }
    let mut product: Option<String> = None;
    let mut path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_ref();
        if arg == "--serve" {
            return Role::Serve;
        }
        if arg == "--confirm" {
            return Role::Confirm;
        }
        if arg == "--product" {
            product = args.get(i + 1).map(|a| a.as_ref().to_string());
            i += 2;
            continue;
        }
        if let Some(name) = arg.strip_prefix("--product=") {
            product = Some(name.to_string());
        } else if !arg.starts_with('-') && path.is_none() {
            path = Some(PathBuf::from(arg));
        }
        i += 1;
    }
    Role::Window {
        product: product.unwrap_or_else(|| DEFAULT_PRODUCT.to_string()),
        path,
    }
}
