//! MCP over a unix socket, and the stdio shim that bridges a client to it.
//!
//! MCP's transport is newline-delimited JSON-RPC 2.0: one message per line.
//! That is simple enough to speak directly, so there is no framework here.
//!
//! The GUI owns a socket; `sheetz mcp` is the process an MCP client actually
//! launches, and it just copies lines between its stdin/stdout and that
//! socket. The GUI therefore never gives up its own stdio, and the workbook
//! the model edits is the one on screen.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::mcp::tools;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Where the socket lives. `$XDG_RUNTIME_DIR` is not guaranteed under a
/// desktop launcher (its environment is thinner than a shell's), so fall back
/// to a per-user path in /tmp.
pub fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p.join("sheetz.sock");
        }
    }
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/tmp/sheetz-{uid}.sock"))
}

// One libc call, not worth a dependency.
unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

/// True if something is listening on the socket right now.
pub fn is_listening() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

/// Starts the socket server on a background thread.
///
/// Called unconditionally at app startup — launching from a desktop launcher
/// must be exactly as good as launching from a terminal. If another instance
/// already holds the socket, this one simply does not serve; a socket file
/// left behind by a crash is detected (nothing answers) and replaced.
pub fn spawn_server() -> Option<PathBuf> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        // A live instance owns it; this window is a secondary.
        return None;
    }
    // Nothing answered: any file here is stale.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sheetz: could not bind {}: {e}", path.display());
            return None;
        }
    };
    std::thread::Builder::new()
        .name("sheetz-mcp".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    if let Err(e) = serve_connection(stream) {
                        eprintln!("sheetz: mcp connection ended: {e}");
                    }
                });
            }
        })
        .ok()?;
    Some(path)
}

/// Answers JSON-RPC on one accepted connection until the peer hangs up.
fn serve_connection(stream: UnixStream) -> std::io::Result<()> {
    let mut out = stream.try_clone()?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_message(&line) {
            writeln!(out, "{response}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Handles one JSON-RPC message. Returns None for notifications.
pub fn handle_message(line: &str) -> Option<Value> {
    let msg: Value = serde_json::from_str(line).ok()?;
    // No id means a notification: act on nothing, answer nothing.
    let id = msg.get("id").filter(|v| !v.is_null())?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let reply = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "sheetz", "version": env!("CARGO_PKG_VERSION") },
            // Clients surface this to the model before any tool is called.
            "instructions": crate::mcp::skill::instructions(),
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            // Per MCP, a tool that fails is a *result* with isError set — the
            // model is meant to read the message and try something else.
            match crate::mcp::bridge::call(name, args) {
                Ok(value) => Ok(json!({
                    "content": [{ "type": "text", "text": render(&value) }],
                    "isError": false,
                })),
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true,
                })),
            }
        }
        other => Err((-32601, format!("method not found: {other}"))),
    };

    Some(match reply {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": {"code": code, "message": message}
        }),
    })
}

/// Tool results go back as text: a bare string stays as-is, anything else is
/// pretty JSON so the model can read structure without a parser.
fn render(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// `sheetz mcp`: answer the handshake here, and only reach for the app when a
/// tool is actually called.
///
/// Connecting an MCP client must never open a window: clients launch this
/// process at session start and list tools whether or not anything is used.
/// The handshake is the same binary's own answers, so it matches the app's.
/// The first `tools/call` connects, starting the GUI if nothing is listening,
/// and from then on every line is proxied verbatim in both directions.
pub fn stdio_shim() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let mut to_app: Option<UnixStream> = None;
    let mut pump: Option<std::thread::JoinHandle<()>> = None;

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(app) = to_app.as_mut() {
            writeln!(app, "{line}")?;
            app.flush()?;
            continue;
        }
        if method_of(&line) == "tools/call" {
            let mut app = connect_to_app()?;
            let from_app = handshake(&mut app)?;
            // Socket → stdout on its own thread; stdin → socket stays here.
            pump = Some(std::thread::spawn(move || pump_to_stdout(from_app)));
            writeln!(app, "{line}")?;
            app.flush()?;
            to_app = Some(app);
            continue;
        }
        // initialize, ping, tools/list — answered locally. tools/call never
        // gets here, so the app-side branch of this handler is unreachable.
        if let Some(response) = handle_message(&line) {
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }

    // The client closed stdin: say so on the socket, then let the pump finish
    // writing whatever the app still owes before this process goes away.
    if let Some(app) = to_app {
        let _ = app.shutdown(std::net::Shutdown::Write);
    }
    if let Some(pump) = pump {
        let _ = pump.join();
    }
    Ok(())
}

fn method_of(line: &str) -> String {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|m| m.get("method").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default()
}

/// Connects, starting the GUI if nothing is listening yet.
fn connect_to_app() -> std::io::Result<UnixStream> {
    let path = socket_path();
    match UnixStream::connect(&path) {
        Ok(s) => Ok(s),
        Err(_) => {
            launch_gui();
            wait_for_socket(&path)
        }
    }
}

/// Replays `initialize` to the app under an id no client uses, and swallows
/// everything up to its answer. Returns the reader to keep proxying with, so
/// no buffered line is lost.
fn handshake(app: &mut UnixStream) -> std::io::Result<BufReader<UnixStream>> {
    const ID: &str = "shim-init";
    let request = json!({
        "jsonrpc": "2.0", "id": ID, "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "sheetz-shim", "version": env!("CARGO_PKG_VERSION") },
        }
    });
    writeln!(app, "{request}")?;
    app.flush()?;

    let mut reader = BufReader::new(app.try_clone()?);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::other("the Sheetz window closed"));
        }
        let id = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|m| m.get("id").and_then(Value::as_str).map(str::to_owned));
        if id.as_deref() == Some(ID) {
            return Ok(reader);
        }
    }
}

fn pump_to_stdout(mut from_app: BufReader<UnixStream>) {
    let mut stdout = std::io::stdout().lock();
    let mut line = String::new();
    loop {
        line.clear();
        match from_app.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if stdout.write_all(line.as_bytes()).is_err() || stdout.flush().is_err() {
                    break;
                }
            }
        }
    }
    // The app went away; a client waiting on stdin should not hang.
    std::process::exit(0);
}

fn launch_gui() {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sheetz"));
    let _ = std::process::Command::new(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Waits for the freshly started GUI to bind its socket.
fn wait_for_socket(path: &PathBuf) -> std::io::Result<UnixStream> {
    for _ in 0..100 {
        if let Ok(s) = UnixStream::connect(path) {
            return Ok(s);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(std::io::Error::other(
        "timed out waiting for the Sheetz window to start",
    ))
}

/// Reads exactly one line; used by tests and the shim's handshake.
pub fn read_line(stream: &mut impl Read) -> std::io::Result<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
}
