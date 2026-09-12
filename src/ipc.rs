//! The app socket's own messages, and how a process reaches the server.
//!
//! Two kinds of line share the socket. JSON-RPC lines are MCP and belong to
//! [`crate::mcp::proto`]; a line carrying an `"op"` key is one of these, the
//! traffic between the server and the processes that show windows:
//!
//! ```text
//! host    {"op":"host","product":"grido"}          a process claims the role
//! open    {"op":"open","id":3,"path":"/a/b.xlsx"}  put this file on screen
//! call    {"op":"call","id":4,"tool":…,"args":…}   run a tool in that window
//! reply   {"op":"reply","id":4,"ok":true,…}        one per request, by id
//! ```
//!
//! `open` with no path means "just come to the front", which is what a second
//! `enclave` with no arguments asks for.

use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How long a process waits for a freshly spawned server to bind the socket.
pub const SERVER_WAIT: Duration = Duration::from_secs(20);

/// One message on the app socket.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// A process offering to be the window for a product.
    Host { product: String },
    /// Show this file (or nothing but the window itself).
    Open { id: Option<u64>, path: Option<PathBuf> },
    /// Run one tool in the product's window.
    Call { id: u64, tool: String, args: Value },
    /// The answer to one request, matched by id.
    Reply {
        id: Option<u64>,
        result: Result<Value, String>,
    },
}

impl Msg {
    /// The single line this message travels as.
    pub fn encode(&self) -> String {
        let value = match self {
            Msg::Host { product } => json!({"op": "host", "product": product}),
            Msg::Open { id, path } => {
                let mut v = json!({"op": "open"});
                if let Some(id) = id {
                    v["id"] = json!(id);
                }
                if let Some(path) = path {
                    v["path"] = json!(path.to_string_lossy());
                }
                v
            }
            Msg::Call { id, tool, args } => {
                json!({"op": "call", "id": id, "tool": tool, "args": args})
            }
            Msg::Reply { id, result } => {
                let mut v = match result {
                    Ok(result) => json!({"op": "reply", "ok": true, "result": result}),
                    Err(error) => json!({"op": "reply", "ok": false, "error": error}),
                };
                if let Some(id) = id {
                    v["id"] = json!(id);
                }
                v
            }
        };
        value.to_string()
    }

    /// Reads one back. `None` for anything that is not an op line — an MCP
    /// message on the same socket, or a line from a newer version than this.
    pub fn parse(line: &str) -> Option<Msg> {
        let value: Value = serde_json::from_str(line).ok()?;
        let op = value.get("op")?.as_str()?;
        let id = value.get("id").and_then(Value::as_u64);
        match op {
            "host" => Some(Msg::Host {
                product: value.get("product")?.as_str()?.to_string(),
            }),
            "open" => Some(Msg::Open {
                id,
                path: value
                    .get("path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from),
            }),
            "call" => Some(Msg::Call {
                id: id?,
                tool: value.get("tool")?.as_str()?.to_string(),
                args: value.get("args").cloned().unwrap_or_else(|| json!({})),
            }),
            "reply" => {
                let result = if value.get("ok").and_then(Value::as_bool) == Some(true) {
                    Ok(value.get("result").cloned().unwrap_or(Value::Null))
                } else {
                    Err(value
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("no reason given")
                        .to_string())
                };
                Some(Msg::Reply { id, result })
            }
            _ => None,
        }
    }
}

/// True when a line is app-socket traffic rather than MCP.
pub fn is_op(line: &str) -> bool {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("op").and_then(Value::as_str).map(str::to_owned))
        .is_some()
}

/// Sends one message and flushes it, so the peer sees it now.
pub fn send(stream: &mut impl Write, msg: &Msg) -> std::io::Result<()> {
    writeln!(stream, "{}", msg.encode())?;
    stream.flush()
}

/// Reads lines until one is a message. `Ok(None)` means the peer hung up.
pub fn recv(reader: &mut impl BufRead) -> std::io::Result<Option<Msg>> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if let Some(msg) = Msg::parse(&line) {
            return Ok(Some(msg));
        }
    }
}

/// A connection to the server, starting one if nothing is listening.
///
/// Every role that is not the server itself goes through here: the stdio shim
/// before it forwards a call, and a product window before it claims its role.
pub fn ensure_server() -> std::io::Result<UnixStream> {
    ensure_server_at(&crate::mcp::proto::socket_path(), spawn_server, SERVER_WAIT)
}

/// The same, with the socket, the way to start a server, and the patience all
/// passed in — which is how the tests drive it without a window anywhere.
pub fn ensure_server_at(
    path: &Path,
    spawn: impl FnOnce(),
    wait: Duration,
) -> std::io::Result<UnixStream> {
    if let Ok(stream) = UnixStream::connect(path) {
        return Ok(stream);
    }
    spawn();
    wait_for_socket(path, wait)
}

/// Polls until something answers on the socket.
pub fn wait_for_socket(path: &Path, wait: Duration) -> std::io::Result<UnixStream> {
    let deadline = Instant::now() + wait;
    loop {
        if let Ok(stream) = UnixStream::connect(path) {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::other(format!(
                "the enclave server did not start within {} s",
                wait.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Starts `enclave --serve`: the process that owns the socket, the windows'
/// registry and the confirmation dialog. It has no window of its own.
pub fn spawn_server() {
    spawn_self(&["--serve"]);
}

/// Starts a window process for one product.
pub fn spawn_product(product: &str) {
    spawn_self(&["--product", product]);
}

fn spawn_self(args: &[&str]) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("enclave"));
    let _ = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
