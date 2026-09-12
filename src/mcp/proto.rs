//! MCP over a unix socket, and the stdio shim that bridges a client to it.
//!
//! MCP's transport is newline-delimited JSON-RPC 2.0: one message per line.
//! That is simple enough to speak directly, so there is no framework here.
//!
//! The server owns a socket; `enclave mcp` is the process an MCP client
//! actually launches. It answers the handshake and the enclave's read-only
//! tools itself, and copies anything that acts — a call into a product, a file
//! going to a colleague — between its stdin/stdout and that socket. The server
//! therefore never gives up its own stdio, it is the one place a confirmation
//! can be asked for, and the workbook the model edits is the one on screen.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Where the socket lives. `$XDG_RUNTIME_DIR` is not guaranteed under a
/// desktop launcher (its environment is thinner than a shell's), so fall back
/// to a per-user path in /tmp.
pub fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p.join("enclave.sock");
        }
    }
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/tmp/enclave-{uid}.sock"))
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

/// Takes the socket, which is what makes a process *the* server.
///
/// Binding is the single-instance lock: whoever gets it serves, and anyone who
/// does not has nothing to do. A socket file left behind by a crash answers
/// nothing, so it is replaced rather than respected.
pub fn bind_socket(path: &Path) -> Option<UnixListener> {
    if UnixStream::connect(path).is_ok() {
        // A live server owns it.
        return None;
    }
    let _ = std::fs::remove_file(path);
    match UnixListener::bind(path) {
        Ok(listener) => Some(listener),
        Err(e) => {
            eprintln!("enclave: could not bind {}: {e}", path.display());
            None
        }
    }
}

/// Handles one JSON-RPC message, running tools right here. Returns None for
/// notifications.
pub fn handle_message(line: &str) -> Option<Value> {
    handle_message_with(line, &dispatch)
}

/// The same, with the tool runner passed in.
///
/// The server hands in its own: one that asks the user first and forwards to
/// whichever window holds the product. The protocol above it — handshake,
/// tool list, what an error looks like — is identical either way, which is why
/// a client cannot tell the shim from the server.
pub fn handle_message_with(
    line: &str,
    run: &(dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync),
) -> Option<Value> {
    let msg: Value = serde_json::from_str(line).ok()?;
    // No id means a notification: act on nothing, answer nothing.
    let id = msg.get("id").filter(|v| !v.is_null())?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let reply = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "enclave", "version": env!("CARGO_PKG_VERSION") },
            // Clients surface this to the model before any tool is called.
            "instructions": instructions(),
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            // Per MCP, a tool that fails is a *result* with isError set — the
            // model is meant to read the message and try something else.
            match run(name, args) {
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

/// Every tool this server advertises: the enclave's own first, then the app's.
pub fn definitions() -> Vec<Value> {
    let mut tools = crate::mcp::tools::definitions();
    tools.extend(grido::mcp::tools::definitions());
    tools
}

/// Runs a tool by its prefix: `enclave_` here, `grido_` on the UI thread of
/// this same process.
///
/// A name with neither prefix is refused rather than guessed at, so a typo
/// never reaches an app — and never opens a window.
fn dispatch(name: &str, args: Value) -> Result<Value, String> {
    if name.starts_with(crate::mcp::tools::PREFIX) {
        return crate::mcp::tools::call(name, &args);
    }
    if name.starts_with(grido::mcp::tools::PREFIX) {
        return crate::mcp::bridge::call(name, args);
    }
    Err(format!("no such tool: {name}"))
}

/// What the model is told before it calls anything. One server carries the
/// network and the apps on it, so say which is which.
fn instructions() -> String {
    format!(
        "Enclave is the user's private network and the apps that run on it. \
         Every tool is named product_verb. The enclave_ tools answer from this \
         computer's daemon and open nothing: enclave_status for the enclaves \
         it is on, enclave_computers for who is reachable, enclave_send_file \
         to drop a file straight onto one of their computers. The grido_ tools \
         drive Grido, the spreadsheet; calling one brings its window up.\n\n{}",
        grido::mcp::skill::instructions()
    )
}

/// Tool results go back as text: a bare string stays as-is, anything else is
/// pretty JSON so the model can read structure without a parser.
fn render(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// `enclave mcp`: answer here whatever can be answered here, and reach for the
/// server for everything that acts.
///
/// Connecting an MCP client must never open a window: clients launch this
/// process at session start and list tools whether or not anything is used.
/// The handshake is the same binary's own answers, so it matches the server's,
/// and reading the enclave — which enclaves, which computers — is served
/// straight from the daemon with nothing on screen. The first call that *does*
/// something connects to the server, starting it if nothing is listening,
/// because that is the process that can ask the user first.
pub fn stdio_shim() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let mut to_app: Option<UnixStream> = None;
    let mut pump: Option<std::thread::JoinHandle<()>> = None;

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if needs_server(&line) {
            if to_app.is_none() {
                let mut app = crate::ipc::ensure_server()?;
                let from_app = handshake(&mut app)?;
                // Socket → stdout on its own thread; stdin → socket stays here.
                pump = Some(std::thread::spawn(move || pump_to_stdout(from_app)));
                to_app = Some(app);
            }
            let app = to_app.as_mut().expect("connected");
            writeln!(app, "{line}")?;
            app.flush()?;
            continue;
        }
        // initialize, ping, tools/list and the read-only enclave_ calls: all
        // answered here, windowless.
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

/// True when this line has to go to the server: a call into a product's
/// window, or anything that acts on the user's behalf. Both need the server —
/// one for the window, both for the confirmation. Everything else the shim
/// answers itself.
pub fn needs_server(line: &str) -> bool {
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if msg.get("method").and_then(Value::as_str) != Some("tools/call") {
        return false;
    }
    msg.pointer("/params/name")
        .and_then(Value::as_str)
        .is_some_and(|name| {
            name.starts_with(grido::mcp::tools::PREFIX) || crate::confirm::acting(name)
        })
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
            "clientInfo": { "name": "enclave-shim", "version": env!("CARGO_PKG_VERSION") },
        }
    });
    writeln!(app, "{request}")?;
    app.flush()?;

    let mut reader = BufReader::new(app.try_clone()?);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::other("the enclave server closed"));
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
    // The server went away; a client waiting on stdin should not hang.
    std::process::exit(0);
}

/// Reads exactly one line; used by tests and the shim's handshake.
pub fn read_line(stream: &mut impl Read) -> std::io::Result<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
}
