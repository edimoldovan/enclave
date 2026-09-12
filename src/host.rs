//! The process that shows a product, and how it hands over.
//!
//! Opening a file must feel like one app, however many times it is launched.
//! So a launch does not assume it owns anything: it makes sure the server is
//! up, then claims the role for its product. Exactly one process can hold a
//! role, so the second `enclave sales.xlsx` is refused — and instead of opening
//! a second, half-connected window, it asks the one already on screen to open
//! the file and come to the front, then exits.
//!
//! Once a process holds the role, the server forwards every tool call for that
//! product down this connection, and the answers go back the same way.

use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::ipc::{self, Msg};

/// How long to keep trying to get the role back if the server goes away.
const RECONNECT_TRIES: usize = 10;
const RECONNECT_PAUSE: Duration = Duration::from_secs(2);

/// How persistently a handed-over file is pushed at a busy window.
const OPEN_TRIES: usize = 5;
const OPEN_PAUSE: Duration = Duration::from_millis(300);

/// What a launch decided to do.
pub enum Outcome {
    /// A window was already there; it took the file. This process is done.
    HandedOff,
    /// Show the UI. `Some` link when the server forwards tool calls here;
    /// `None` when there is no server and this window is on its own.
    Run(Option<Link>),
}

/// A live connection to the server, and the reader that keeps its place in the
/// stream — the registration reply may share a read with what follows it.
pub struct Link {
    out: Arc<Mutex<UnixStream>>,
    reader: BufReader<UnixStream>,
}

impl Link {
    fn new(stream: UnixStream) -> std::io::Result<Link> {
        Ok(Link {
            reader: BufReader::new(stream.try_clone()?),
            out: Arc::new(Mutex::new(stream)),
        })
    }
}

/// Decides what this launch is: the window, or the messenger.
pub fn start(product: &str, path: Option<&Path>) -> Outcome {
    match ipc::ensure_server() {
        Ok(stream) => start_on(stream, product, path),
        Err(e) => {
            // No server and none would start. Better a window with no assistant
            // than no window at all.
            eprintln!("enclave: {e} — this window runs on its own");
            Outcome::Run(None)
        }
    }
}

/// The same decision, on a connection already in hand — which is how the tests
/// drive it against a server with no screen.
pub fn start_on(stream: UnixStream, product: &str, path: Option<&Path>) -> Outcome {
    let mut link = match Link::new(stream) {
        Ok(link) => link,
        Err(e) => {
            eprintln!("enclave: {e} — this window runs on its own");
            return Outcome::Run(None);
        }
    };
    match claim(&mut link, product) {
        Ok(()) => Outcome::Run(Some(link)),
        // Another window holds the role. Give it the file and go.
        Err(_) => match hand_off(&mut link, path) {
            HandOff::Done => Outcome::HandedOff,
            // The window answered and said no. Say why; a second window would
            // only be the dead one this whole handover exists to avoid.
            HandOff::Refused(why) => {
                eprintln!("enclave: {why}");
                Outcome::HandedOff
            }
            HandOff::Unreachable => Outcome::Run(None),
        },
    }
}

/// How asking the other window went.
enum HandOff {
    Done,
    /// It answered, and the answer was no.
    Refused(String),
    /// Nothing answered at all.
    Unreachable,
}

/// Asks for the role. `Err` means someone else has it.
fn claim(link: &mut Link, product: &str) -> Result<(), String> {
    let msg = Msg::Host {
        product: product.to_string(),
    };
    {
        let mut out = link.out.lock().map_err(|_| "link poisoned".to_string())?;
        ipc::send(&mut *out, &msg).map_err(|e| e.to_string())?;
    }
    match ipc::recv(&mut link.reader) {
        Ok(Some(Msg::Reply { result, .. })) => result.map(|_| ()),
        Ok(Some(_)) | Ok(None) => Err("the server said nothing".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Asks the window that holds the role to open this file and come forward.
fn hand_off(link: &mut Link, path: Option<&Path>) -> HandOff {
    let msg = Msg::Open {
        id: None,
        path: path.map(absolute),
    };
    match link.out.lock() {
        Ok(mut out) => {
            if let Err(e) = ipc::send(&mut *out, &msg) {
                return HandOff::Refused(e.to_string());
            }
        }
        Err(_) => return HandOff::Unreachable,
    }
    match ipc::recv(&mut link.reader) {
        Ok(Some(Msg::Reply {
            result: Ok(_ok), ..
        })) => HandOff::Done,
        Ok(Some(Msg::Reply {
            result: Err(why), ..
        })) => HandOff::Refused(why),
        _ => HandOff::Unreachable,
    }
}

/// Starts serving the server's requests on a background thread.
///
/// Call this once the UI thread has a bridge to run tool calls on, since that
/// is where every request ends up.
pub fn serve(product: String, link: Link) {
    std::thread::Builder::new()
        .name("enclave-host".into())
        .spawn(move || {
            let mut link = link;
            for attempt in 0..=RECONNECT_TRIES {
                pump(&mut link);
                // The server went away. The window stays; get the role back so
                // an assistant can reach this workbook again.
                if attempt == RECONNECT_TRIES {
                    break;
                }
                std::thread::sleep(RECONNECT_PAUSE);
                let Ok(stream) = ipc::ensure_server() else {
                    continue;
                };
                let Ok(fresh) = Link::new(stream) else {
                    continue;
                };
                link = fresh;
                if claim(&mut link, &product).is_err() {
                    // Another window took the role while we were away. Stay
                    // open, stay quiet.
                    break;
                }
            }
        })
        .ok();
}

/// Reads requests until the server hangs up.
fn pump(link: &mut Link) {
    loop {
        let msg = match ipc::recv(&mut link.reader) {
            Ok(Some(msg)) => msg,
            Ok(None) | Err(_) => return,
        };
        let out = link.out.clone();
        // One thread per request: a tool call waits on the UI thread, and an
        // "open" arriving meanwhile should not queue behind it.
        std::thread::spawn(move || {
            let (id, result) = match msg {
                Msg::Call { id, tool, args } => (Some(id), crate::mcp::bridge::call(&tool, args)),
                Msg::Open { id, path } => (id, open(path)),
                _ => return,
            };
            if let Ok(mut out) = out.lock() {
                let _ = ipc::send(&mut *out, &Msg::Reply { id, result });
            }
        });
    }
}

/// Opens a file in this window and brings it forward. No path means the launch
/// carried none: the window itself is what was asked for.
///
/// The window comes forward first, whatever happens next — someone typed a
/// command and is waiting to see something. The open is retried a few times
/// because the one thing that refuses it, a cell open for editing, clears in a
/// second or two.
fn open(path: Option<PathBuf>) -> Result<Value, String> {
    crate::mcp::bridge::front();
    let Some(path) = path else {
        return Ok(json!("here"));
    };
    let mut refusal = "the window did not answer".to_string();
    for attempt in 0..OPEN_TRIES {
        if attempt > 0 {
            std::thread::sleep(OPEN_PAUSE);
        }
        match crate::mcp::bridge::call(
            "grido_workbook_open",
            json!({ "path": path.to_string_lossy() }),
        ) {
            Ok(value) => return Ok(value),
            Err(e) => refusal = e,
        }
    }
    Err(refusal)
}

/// An absolute path without requiring the file to exist yet.
fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|dir| dir.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}
