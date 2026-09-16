//! The app socket: its messages, and how a process gets a server to talk to.

use std::io::BufReader;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use enclave::ipc::{self, Msg};
use serde_json::json;

/// A scratch runtime directory, standing in for `$XDG_RUNTIME_DIR`.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("enclave-tests")
        .join(format!("ipc-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    dir
}

#[test]
fn every_message_survives_the_round_trip() {
    let messages = [
        Msg::Host {
            product: "grido".to_string(),
        },
        Msg::Open {
            id: Some(7),
            path: Some(PathBuf::from("/home/ed/q3.xlsx")),
        },
        Msg::Open {
            id: None,
            path: None,
        },
        Msg::View {
            id: Some(1),
            account: Some("ed@acme.com".to_string()),
            message: Some("18f3a2c9b1".to_string()),
            compose: false,
        },
        Msg::View {
            id: Some(2),
            account: Some("ed@acme.com".to_string()),
            message: None,
            compose: false,
        },
        Msg::View {
            id: None,
            account: None,
            message: None,
            compose: false,
        },
        // A reply to that message, rather than the message.
        Msg::View {
            id: Some(4),
            account: Some("ed@acme.com".to_string()),
            message: Some("18f3a2c9b1".to_string()),
            compose: true,
        },
        Msg::Call {
            id: 3,
            tool: "grido_cell_set".to_string(),
            args: json!({"cell": "A1", "value": 5}),
        },
        Msg::Reply {
            id: Some(3),
            result: Ok(json!({"ok": "done"})),
        },
        Msg::Reply {
            id: None,
            result: Err("a grido window is already running".to_string()),
        },
    ];
    for msg in messages {
        let line = msg.encode();
        assert!(!line.contains('\n'), "one message is one line: {line}");
        assert!(ipc::is_op(&line), "{line} should be app-socket traffic");
        assert_eq!(Msg::parse(&line), Some(msg.clone()), "on {line}");
    }
}

/// The op messages and MCP share one socket, so each has to be able to say it
/// is not the other.
#[test]
fn an_mcp_line_is_not_an_op() {
    let rpc = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}).to_string();
    assert!(!ipc::is_op(&rpc));
    assert!(Msg::parse(&rpc).is_none());
    assert!(Msg::parse("not json at all").is_none());
    assert!(Msg::parse(&json!({"op": "invented-later"}).to_string()).is_none());
}

#[test]
fn an_open_carries_the_path_as_given() {
    let line = Msg::Open {
        id: Some(1),
        path: Some(PathBuf::from("/home/ed/sales 2026.xlsx")),
    }
    .encode();
    let parsed: serde_json::Value = serde_json::from_str(&line).expect("json");
    assert_eq!(parsed["op"], "open");
    assert_eq!(parsed["id"], 1);
    assert_eq!(parsed["path"], "/home/ed/sales 2026.xlsx");
}

#[test]
fn a_refusal_keeps_its_words() {
    let line = json!({"op": "reply", "id": 4, "ok": false, "error": "denied by the user"}).to_string();
    let Some(Msg::Reply { id, result }) = Msg::parse(&line) else {
        panic!("should parse as a reply");
    };
    assert_eq!(id, Some(4));
    assert_eq!(result, Err("denied by the user".to_string()));
}

#[test]
fn messages_travel_over_a_socket_and_skip_what_is_not_one() {
    let (mut a, b) = UnixStream::pair().expect("a socket pair");
    let mut reader = BufReader::new(b);

    // An MCP line on the same socket must not derail a reader waiting for ops.
    let rpc = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}).to_string();
    use std::io::Write;
    writeln!(a, "{rpc}").expect("write");
    ipc::send(
        &mut a,
        &Msg::Host {
            product: "grido".to_string(),
        },
    )
    .expect("send");

    assert_eq!(
        ipc::recv(&mut reader).expect("recv"),
        Some(Msg::Host {
            product: "grido".to_string()
        })
    );

    // A closed peer reads as "nothing more", not as an error.
    drop(a);
    assert_eq!(ipc::recv(&mut reader).expect("recv"), None);
}

/// The spawn-and-wait every role does before it can talk to anything.
#[test]
fn a_server_is_started_and_waited_for() {
    let path = scratch("spawn").join("enclave.sock");
    let _ = std::fs::remove_file(&path);
    let asked = Arc::new(AtomicBool::new(false));

    let spawned = asked.clone();
    let bind_at = path.clone();
    let stream = ipc::ensure_server_at(
        &path,
        move || {
            spawned.store(true, Ordering::SeqCst);
            // Stand in for `enclave --serve`: a little late, as a real process is.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(120));
                let listener = UnixListener::bind(&bind_at).expect("bind");
                let _held = listener.accept();
                std::thread::sleep(Duration::from_millis(50));
            });
        },
        Duration::from_secs(5),
    )
    .expect("a connection to the server");

    assert!(asked.load(Ordering::SeqCst), "a server should be started");
    drop(stream);
    let _ = std::fs::remove_file(&path);
}

/// A server already listening is used as it is — a second one is never started.
#[test]
fn a_running_server_is_left_alone() {
    let path = scratch("running").join("enclave.sock");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind");
    let asked = Arc::new(AtomicBool::new(false));

    let spawned = asked.clone();
    let stream = ipc::ensure_server_at(
        &path,
        move || spawned.store(true, Ordering::SeqCst),
        Duration::from_millis(200),
    )
    .expect("a connection to the server");

    assert!(
        !asked.load(Ordering::SeqCst),
        "nothing should be started when a server answers"
    );
    drop(stream);
    drop(listener);
    let _ = std::fs::remove_file(&path);
}

/// A server that never comes up is an error with words in it, not a hang.
#[test]
fn waiting_gives_up_and_says_so() {
    let path = scratch("never").join("enclave.sock");
    let _ = std::fs::remove_file(&path);
    let error = ipc::ensure_server_at(&path, || {}, Duration::from_millis(150))
        .expect_err("nothing is listening");
    assert!(
        error.to_string().contains("did not start"),
        "got {error:?}"
    );
}
