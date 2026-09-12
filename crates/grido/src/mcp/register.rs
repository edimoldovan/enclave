//! Registering Enclave with MCP clients.
//!
//! A listening socket is useless if the assistant does not know Enclave
//! exists, and registering a server normally means hand-editing a client's
//! JSON config — exactly the terminal step this is meant to avoid. So Enclave
//! writes the entry itself, from the installer or from the File view.
//!
//! Existing config is merged, never replaced, and a `.bak` is left behind.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// A client config we know how to write, and whether it is present.
pub struct Client {
    pub name: &'static str,
    pub path: PathBuf,
    pub registered: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Config files of the MCP clients we support, whether or not they exist yet.
pub fn known_clients() -> Vec<Client> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let candidates: Vec<(&'static str, PathBuf)> = vec![
        (
            "Claude Desktop",
            home.join(".config/Claude/claude_desktop_config.json"),
        ),
        ("Claude Code", home.join(".claude.json")),
    ];
    candidates
        .into_iter()
        .map(|(name, path)| {
            let registered = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .map(|v| v.pointer("/mcpServers/enclave").is_some())
                .unwrap_or(false);
            Client {
                name,
                path,
                registered,
            }
        })
        .collect()
}

/// The command an MCP client should run to reach this install.
fn server_entry() -> Result<Value> {
    let exe = std::env::current_exe().context("cannot find the enclave binary")?;
    Ok(json!({
        "command": exe.display().to_string(),
        "args": ["mcp"],
    }))
}

/// The command line an entry runs, command and args together.
fn command_line(entry: &Value) -> String {
    let mut text = entry
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(args) = entry.get("args").and_then(Value::as_array) {
        for arg in args.iter().filter_map(Value::as_str) {
            text.push(' ');
            text.push_str(arg);
        }
    }
    text
}

/// True when the entry runs something out of the directory this binary lives
/// in — a wrapper around the same install.
fn from_our_directory(entry: &Value) -> bool {
    let text = command_line(entry);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.display().to_string()))
        .is_some_and(|dir| !dir.is_empty() && text.contains(&dir))
}

/// True when the entry's command is a binary of this name.
fn runs_binary(entry: &Value, binary: &str) -> bool {
    let command = entry
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    std::path::Path::new(command)
        .file_name()
        .is_some_and(|name| name == binary)
}

/// True when an entry under `name` is one this install superseded.
///
/// `sheetz` and `grido` are what this server was called before; an `enclave`
/// entry running `enclave-mcp` is the Go shim this binary replaces. Each is
/// only claimed when it is ours — a binary of that name, or something out of
/// this binary's own directory — so a server someone else happens to have
/// called `grido` is left alone.
fn is_superseded(name: &str, entry: &Value) -> bool {
    match name {
        "sheetz" => runs_binary(entry, "sheetz") || from_our_directory(entry),
        "grido" => runs_binary(entry, "grido") || from_our_directory(entry),
        "enclave" => runs_binary(entry, "enclave-mcp"),
        _ => false,
    }
}

/// The entries that must not survive this registration.
fn superseded(root: &Value) -> Vec<&'static str> {
    ["sheetz", "grido", "enclave"]
        .into_iter()
        .filter(|name| {
            root.pointer(&format!("/mcpServers/{name}"))
                .is_some_and(|entry| is_superseded(name, entry))
        })
        .collect()
}

/// True when this config already points at this exact binary — nothing to do.
///
/// Only the keys registration writes are compared. A client that keeps its own
/// (`type`, `env`) alongside them is already correct, and rewriting the file to
/// drop them would be a change nobody asked for.
pub fn is_current(path: &PathBuf) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    // A superseded entry still has to go, however right the new one is.
    if !superseded(&root).is_empty() {
        return false;
    }
    match (root.pointer("/mcpServers/enclave"), server_entry().ok()) {
        (Some(existing), Some(wanted)) => ["command", "args"]
            .iter()
            .all(|k| existing.get(k) == wanted.get(k)),
        _ => false,
    }
}

/// Adds (or refreshes) the `enclave` entry in one client config, and drops the
/// entries this install superseded — the `sheetz` and `grido` servers it used
/// to be called, and the Go `enclave-mcp` shim it replaces.
///
/// Only those keys are touched: every other setting is preserved, key order
/// is kept (serde_json's `preserve_order`), and the previous contents are left
/// as `<file>.bak`. Returns false when the entry was already correct, so
/// startup registration is a no-op after the first run.
pub fn register_one(path: &PathBuf) -> Result<bool> {
    if is_current(path) {
        return Ok(false);
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut root: Value = if existing.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&existing)
            .with_context(|| format!("{} is not valid JSON", path.display()))?
    };
    let stale = superseded(&root);
    if !existing.trim().is_empty() {
        let _ = std::fs::write(path.with_extension("json.bak"), &existing);
    }
    if !root.is_object() {
        root = json!({});
    }
    let servers = root
        .as_object_mut()
        .expect("object")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    let servers = servers.as_object_mut().expect("object");
    // Written first, so an `enclave` entry that was the old Go shim is
    // refreshed in place rather than dropped and re-added at the end.
    servers.insert("enclave".to_string(), server_entry()?);
    for name in stale.iter().filter(|name| **name != "enclave") {
        // shift_remove, not remove: the rest of the list keeps its order.
        servers.shift_remove(*name);
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, serde_json::to_string_pretty(&root)?)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(true)
}

/// Registers with every installed client, quietly.
///
/// Called on app startup so that *running Enclave* is the only thing a user
/// ever has to do — no installer, no terminal, no editing JSON by hand. It
/// rewrites nothing once the entry is correct.
pub fn register_on_startup() {
    for client in known_clients() {
        if client.path.exists() {
            let _ = register_one(&client.path);
        }
    }
}

/// Registers with every client config that already exists.
///
/// Returns what was written and what was skipped, so the caller can tell the
/// user rather than claiming success silently.
pub fn register_all() -> (Vec<String>, Vec<String>) {
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for client in known_clients() {
        if !client.path.exists() {
            skipped.push(format!("{} (not installed)", client.name));
            continue;
        }
        match register_one(&client.path) {
            Ok(true) => done.push(client.name.to_string()),
            Ok(false) => done.push(format!("{} (already connected)", client.name)),
            Err(e) => skipped.push(format!("{}: {e}", client.name)),
        }
    }
    (done, skipped)
}
