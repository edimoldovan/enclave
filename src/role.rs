//! Which role a command line asks for.
//!
//! One executable, several jobs, and the arguments alone decide which — so the
//! binary an assistant launches, the server it starts, the dialog that server
//! puts on screen, and the window a person opens a file with are the same file
//! on disk.

use std::path::PathBuf;

/// The product a bare `enclave file.xlsx` means.
pub const DEFAULT_PRODUCT: &str = "grido";

/// The word that makes a launch *be* the Post window rather than ask for one.
///
/// Nobody types it: a launcher writes it, and the process that reads it back is
/// the detached one that holds the event loop. It is what keeps the terminal's
/// process out of the window business entirely.
pub const WINDOW_ARG: &str = "--window";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// `enclave mcp` — speak MCP on stdin/stdout.
    Shim,
    /// `enclave register` — wire this install into the assistants found here.
    Register,
    /// `enclave post <verb> …` — one mail verb, from a terminal. The third door
    /// beside the chat and the windows, off the same palette.
    Post { words: Vec<String> },
    /// `enclave post`, `enclave post list <email>`, `enclave post read <email>
    /// <id>` — the three mail verbs that are views rather than answers. Nobody
    /// reads email in a terminal, so these open the window at that view: hand
    /// it to the window that is up, or start one, and give the prompt back.
    PostOpen { view: post_ui::View },
    /// `enclave post --window <view>` — be that window. Only a launcher writes
    /// this line, and the process that reads it is detached from whoever did.
    PostWindow { view: post_ui::View },
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
        // A product's name, then its verb: everything after "post" belongs to
        // the mail palette, flags included. Three of those verbs are views —
        // the palette itself says which — and open the window; the rest answer
        // in the terminal, where they already say the right thing.
        Some("post") => {
            let words: Vec<String> = args[1..].iter().map(|a| a.as_ref().to_string()).collect();
            // The launcher's own line first: `post --window <view>` is the
            // process that holds the event loop.
            if let Some((first, rest)) = words.split_first()
                && first == WINDOW_ARG
                && let Some(view) = post_ui::View::of_words(rest)
            {
                return Role::PostWindow { view };
            }
            return match post_ui::View::of_words(&words) {
                Some(view) => Role::PostOpen { view },
                None => Role::Post { words },
            };
        }
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
