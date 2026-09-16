//! `enclave mcp` end to end, as a client runs it: a real process, over pipes.
//!
//! The point of this one is what does *not* happen. Connecting an assistant,
//! listing the tools and reading the enclave must not start a server, must not
//! want a screen, and must leave nothing behind.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const EXE: &str = env!("CARGO_BIN_EXE_enclave");

#[test]
fn the_shim_serves_the_handshake_and_the_enclave_without_a_window() {
    let dir = std::env::temp_dir()
        .join("enclave-tests")
        .join(format!("shim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch runtime dir");
    let socket = dir.join("enclave.sock");
    let _ = std::fs::remove_file(&socket);

    let mut child = Command::new(EXE)
        .arg("mcp")
        // A runtime directory of its own: the socket it would use, and the
        // daemon it would ask, are both under here and neither exists.
        .env("XDG_RUNTIME_DIR", &dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("enclave mcp should start");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    for request in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "enclave_status", "arguments": {}}}),
    ] {
        writeln!(stdin, "{request}").expect("write");
    }
    stdin.flush().expect("flush");

    let mut replies = Vec::new();
    for _ in 0..3 {
        let mut line = String::new();
        stdout.read_line(&mut line).expect("a reply");
        replies.push(serde_json::from_str::<Value>(&line).expect("json"));
    }

    assert_eq!(replies[0]["id"], 1);
    assert_eq!(
        replies[0]["result"]["protocolVersion"],
        enclave::mcp::proto::PROTOCOL_VERSION
    );
    assert_eq!(replies[1]["id"], 2);
    assert_eq!(
        replies[1]["result"]["tools"].as_array().expect("tools").len(),
        34,
        "the whole product's tools, listed without opening anything"
    );
    // enclave_status is answered from the daemon, and there is none under this
    // runtime directory — so it comes back as a result that says so, never as a
    // protocol error and never by opening a window.
    assert_eq!(replies[2]["id"], 3);
    assert!(replies[2].get("error").is_none(), "got {}", replies[2]);
    let text = replies[2]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.contains("enclaved is not running"),
        "got {text:?}"
    );

    drop(stdin);
    let status = child.wait().expect("the shim should exit on its own");
    assert!(status.success(), "got {status:?}");

    assert!(
        !socket.exists(),
        "no server should have been started: {} exists",
        socket.display()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Which lines the shim keeps and which it forwards. Everything read-only is
/// answered in the shim; anything that acts goes to the server, because that is
/// the only process that can ask the user first.
#[test]
fn only_acting_calls_need_the_server() {
    let call = |name: &str| {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": name, "arguments": {}}})
        .to_string()
    };
    for kept in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string(),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(),
        json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}).to_string(),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        call("enclave_status"),
        call("enclave_computers"),
        // Mail is a library: the shim runs every free verb itself, so none of
        // them reaches for a server — and neither does the window, which the
        // shim opens down Post's own socket.
        call("post_show"),
        call("post_accounts"),
        call("post_add_account"),
        call("post_list"),
        call("post_read"),
        call("post_mark"),
        call("post_delete"),
        call("post_attachment"),
        // Writing a reply down is free too: nothing has left this computer.
        call("post_draft"),
    ] {
        assert!(
            !enclave::mcp::proto::needs_server(&kept),
            "should be answered in the shim: {kept}"
        );
    }
    for forwarded in [
        call("enclave_send_file"),
        call("grido_workbook_info"),
        call("grido_range_read"),
        call("grido_cell_set"),
        call("grido_rows_delete"),
        // Sending mail is the one mail verb that goes to the server, because
        // the server is the process that can ask the user first.
        call("post_send"),
    ] {
        assert!(
            enclave::mcp::proto::needs_server(&forwarded),
            "should go to the server: {forwarded}"
        );
    }
}
