//! The server, driven over socket pairs with no screen anywhere.
//!
//! Everything the server does apart from drawing the confirm window happens
//! here: a window claiming its role, a second launch handing over its file, a
//! tool call reaching the window that holds the role, and the gate in between.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use enclave::confirm::{Broker, Decision};
use enclave::ipc::{self, Msg};
use enclave::serve::Server;
use serde_json::{json, Value};

/// A server with an empty allowlist, so every acting tool really does ask.
fn server(name: &str) -> Arc<Server> {
    let dir = std::env::temp_dir()
        .join("enclave-tests")
        .join(format!("serve-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let file = dir.join("allowlist.toml");
    let _ = std::fs::remove_file(&file);
    Arc::new(Server::new(Broker::new(file)))
}

/// A connection into the server, served on its own thread as a real one is.
fn connect(server: &Arc<Server>) -> UnixStream {
    let (mine, theirs) = UnixStream::pair().expect("a socket pair");
    let server = server.clone();
    std::thread::spawn(move || {
        let _ = enclave::serve::serve_connection(&server, theirs);
    });
    mine
}

/// A connection that has claimed the grido role, plus the reader to watch it
/// with — this is what a Grido window is, to the server.
fn window(server: &Arc<Server>) -> (UnixStream, BufReader<UnixStream>) {
    let mut out = connect(server);
    let mut reader = BufReader::new(out.try_clone().expect("clone"));
    ipc::send(
        &mut out,
        &Msg::Host {
            product: "grido".to_string(),
        },
    )
    .expect("send");
    match ipc::recv(&mut reader).expect("recv") {
        Some(Msg::Reply { result: Ok(_), .. }) => {}
        other => panic!("the role should be granted, got {other:?}"),
    }
    (out, reader)
}

fn rpc(stream: &mut UnixStream, id: u64, tool: &str, args: Value) {
    let line = json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": tool, "arguments": args}
    });
    writeln!(stream, "{line}").expect("write");
    stream.flush().expect("flush");
}

fn reply_line(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("a reply line");
    serde_json::from_str(&line).expect("json")
}

/// Waits for the broker to have a question to ask.
fn question(server: &Arc<Server>) -> enclave::confirm::Waiting {
    for _ in 0..400 {
        if let Some(w) = server.broker.front() {
            return w;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("nothing was ever asked about");
}

/// Only one process shows a product. That is what stops a second launch from
/// becoming a second, half-connected window.
#[test]
fn one_window_holds_the_role() {
    let server = server("role");
    let (_first, _reader) = window(&server);

    let mut second = connect(&server);
    let mut reader = BufReader::new(second.try_clone().expect("clone"));
    ipc::send(
        &mut second,
        &Msg::Host {
            product: "grido".to_string(),
        },
    )
    .expect("send");
    let Some(Msg::Reply {
        result: Err(why), ..
    }) = ipc::recv(&mut reader).expect("recv")
    else {
        panic!("the second claim should be refused");
    };
    assert!(why.contains("already running"), "got {why:?}");
}

/// `enclave q3.xlsx` while a window is open: the file goes to that window and
/// the process that was launched is done.
#[test]
fn a_second_launch_hands_its_file_over() {
    let server = server("handoff");
    let (mut win, mut win_reader) = window(&server);

    let launch = connect(&server);
    let started = std::thread::spawn(move || {
        enclave::host::start_on(launch, "grido", Some(Path::new("/home/ed/q3.xlsx")))
    });

    let Some(Msg::Open { id, path }) = ipc::recv(&mut win_reader).expect("recv") else {
        panic!("the window should be asked to open the file");
    };
    assert_eq!(path.as_deref(), Some(Path::new("/home/ed/q3.xlsx")));
    ipc::send(
        &mut win,
        &Msg::Reply {
            id,
            result: Ok(json!("opened")),
        },
    )
    .expect("send");

    assert!(
        matches!(
            started.join().expect("the launch thread"),
            enclave::host::Outcome::HandedOff
        ),
        "the second launch should hand over and exit"
    );
}

/// A relative path is made absolute before it is handed over: the window has a
/// different working directory than the terminal that launched this.
#[test]
fn a_handed_over_path_is_absolute() {
    let server = server("absolute");
    let (mut win, mut win_reader) = window(&server);

    let launch = connect(&server);
    let started = std::thread::spawn(move || {
        enclave::host::start_on(launch, "grido", Some(Path::new("book.xlsx")))
    });

    let Some(Msg::Open { id, path }) = ipc::recv(&mut win_reader).expect("recv") else {
        panic!("the window should be asked to open the file");
    };
    let path = path.expect("a path");
    assert!(path.is_absolute(), "got {}", path.display());
    assert!(path.ends_with("book.xlsx"), "got {}", path.display());
    ipc::send(
        &mut win,
        &Msg::Reply {
            id,
            result: Ok(json!("opened")),
        },
    )
    .expect("send");
    let _ = started.join();
}

/// A window that cannot take the file right now still keeps the launch from
/// becoming a second window — that second window is the whole bug.
#[test]
fn a_refused_handover_does_not_open_a_second_window() {
    let server = server("refused");
    let (mut win, mut win_reader) = window(&server);

    let launch = connect(&server);
    let started = std::thread::spawn(move || {
        enclave::host::start_on(launch, "grido", Some(Path::new("/home/ed/q3.xlsx")))
    });

    let Some(Msg::Open { id, .. }) = ipc::recv(&mut win_reader).expect("recv") else {
        panic!("the window should be asked to open the file");
    };
    ipc::send(
        &mut win,
        &Msg::Reply {
            id,
            result: Err("the user is editing a cell right now".to_string()),
        },
    )
    .expect("send");

    assert!(
        matches!(
            started.join().expect("the launch thread"),
            enclave::host::Outcome::HandedOff
        ),
        "a refusal is still a handover, with the reason printed"
    );
}

/// The first launch of the day is the window itself.
#[test]
fn the_first_launch_becomes_the_window() {
    let server = server("first");
    let launch = connect(&server);
    assert!(
        matches!(
            enclave::host::start_on(launch, "grido", None),
            enclave::host::Outcome::Run(Some(_))
        ),
        "it should keep the link and show the UI"
    );
}

/// A tool that only reads goes straight through to the window and its answer
/// comes back as the tool result.
#[test]
fn a_read_tool_reaches_the_window() {
    let server = server("read");
    let (mut win, mut win_reader) = window(&server);
    let mut client = connect(&server);
    let mut client_reader = BufReader::new(client.try_clone().expect("clone"));

    rpc(&mut client, 1, "grido_range_read", json!({"range": "A1:B2"}));

    let Some(Msg::Call { id, tool, args }) = ipc::recv(&mut win_reader).expect("recv") else {
        panic!("the call should reach the window");
    };
    assert_eq!(tool, "grido_range_read");
    assert_eq!(args["range"], "A1:B2");
    ipc::send(
        &mut win,
        &Msg::Reply {
            id: Some(id),
            result: Ok(json!({"cells": [["1", "2"]]})),
        },
    )
    .expect("send");

    let reply = reply_line(&mut client_reader);
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["isError"], false);
    assert!(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("cells"),
        "got {reply}"
    );
}

/// A tool that acts does not reach the window until someone clicks Approve.
#[test]
fn an_acting_tool_waits_for_the_click() {
    let server = server("approve");
    let (mut win, mut win_reader) = window(&server);
    let mut client = connect(&server);
    let mut client_reader = BufReader::new(client.try_clone().expect("clone"));

    rpc(
        &mut client,
        2,
        "grido_cell_set",
        json!({"cell": "A1", "value": 5}),
    );

    // Nothing reaches the window while the question is unanswered.
    let w = question(&server);
    assert_eq!(w.tool, "grido_cell_set");
    win.set_read_timeout(Some(Duration::from_millis(250)))
        .expect("timeout");
    assert!(
        ipc::recv(&mut win_reader).is_err(),
        "the call must not reach the window before it is allowed"
    );

    server.broker.resolve(w.id, Decision::Approve, false);

    win.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let Some(Msg::Call { id, tool, .. }) = ipc::recv(&mut win_reader).expect("recv") else {
        panic!("the approved call should reach the window");
    };
    assert_eq!(tool, "grido_cell_set");
    ipc::send(
        &mut win,
        &Msg::Reply {
            id: Some(id),
            result: Ok(json!("set")),
        },
    )
    .expect("send");

    let reply = reply_line(&mut client_reader);
    assert_eq!(reply["id"], 2);
    assert_eq!(reply["result"]["isError"], false);
}

/// Denied means nothing happens anywhere, and the model is told plainly.
#[test]
fn a_denied_tool_never_reaches_the_window() {
    let server = server("deny");
    let (win, mut win_reader) = window(&server);
    let mut client = connect(&server);
    let mut client_reader = BufReader::new(client.try_clone().expect("clone"));

    rpc(&mut client, 3, "grido_rows_delete", json!({"at": "4:9"}));

    let w = question(&server);
    server.broker.resolve(w.id, Decision::Deny, false);

    let reply = reply_line(&mut client_reader);
    assert_eq!(reply["id"], 3);
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(
        reply["result"]["content"][0]["text"],
        enclave::confirm::DENIED
    );

    win.set_read_timeout(Some(Duration::from_millis(250)))
        .expect("timeout");
    assert!(
        ipc::recv(&mut win_reader).is_err(),
        "a refused call must never reach the window"
    );
}

/// Two calls in flight at once: the second question does not wait for the
/// first one's answer, and each answer comes back under its own request id
/// however the window replies.
#[test]
fn calls_queue_up_and_each_answer_finds_its_call() {
    let server = server("queue");
    let (mut win, mut win_reader) = window(&server);
    let mut client = connect(&server);
    let mut client_reader = BufReader::new(client.try_clone().expect("clone"));

    rpc(&mut client, 10, "grido_cell_set", json!({"cell": "A1"}));
    rpc(&mut client, 11, "grido_cell_set", json!({"cell": "A2"}));
    for _ in 0..400 {
        if server.broker.waiting() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(server.broker.waiting(), 2, "both should be asked about");

    // Approve both, then let the window answer them the other way round.
    let mut calls = Vec::new();
    for _ in 0..2 {
        let w = question(&server);
        server.broker.resolve(w.id, Decision::Approve, false);
        let Some(Msg::Call { id, args, .. }) = ipc::recv(&mut win_reader).expect("recv") else {
            panic!("an approved call should reach the window");
        };
        calls.push((id, args["cell"].as_str().expect("cell").to_string()));
    }
    for (id, cell) in calls.iter().rev() {
        ipc::send(
            &mut win,
            &Msg::Reply {
                id: Some(*id),
                result: Ok(json!(format!("wrote {cell}"))),
            },
        )
        .expect("send");
    }

    let mut answers = std::collections::HashMap::new();
    for _ in 0..2 {
        let reply = reply_line(&mut client_reader);
        let id = reply["id"].as_u64().expect("an id");
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string();
        answers.insert(id, text);
    }
    assert_eq!(answers.get(&10).map(String::as_str), Some("wrote A1"));
    assert_eq!(answers.get(&11).map(String::as_str), Some("wrote A2"));
}

/// The same gate stands in front of the enclave's own acting tool: a denial
/// means the daemon is never asked to send anything.
#[test]
fn sending_a_file_is_asked_about_too() {
    let server = server("send");
    let mut client = connect(&server);
    let mut client_reader = BufReader::new(client.try_clone().expect("clone"));

    rpc(
        &mut client,
        4,
        "enclave_send_file",
        json!({"computer": "nobody", "path": "/home/ed/q3.xlsx"}),
    );

    let w = question(&server);
    assert_eq!(w.tool, "enclave_send_file");
    assert!(w.summary.contains("q3.xlsx"), "got {:?}", w.summary);
    server.broker.resolve(w.id, Decision::Deny, false);

    let reply = reply_line(&mut client_reader);
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(
        reply["result"]["content"][0]["text"],
        enclave::confirm::DENIED
    );
}

/// The handshake and the tool list are the same over the socket as they are in
/// the shim: a client cannot tell which one it is talking to.
#[test]
fn the_server_answers_the_handshake_itself() {
    let server = server("handshake");
    let mut client = connect(&server);
    let mut reader = BufReader::new(client.try_clone().expect("clone"));

    writeln!(
        client,
        "{}",
        json!({"jsonrpc": "2.0", "id": "init", "method": "initialize", "params": {}})
    )
    .expect("write");
    let reply = reply_line(&mut reader);
    assert_eq!(
        reply["result"]["protocolVersion"],
        enclave::mcp::proto::PROTOCOL_VERSION
    );

    writeln!(
        client,
        "{}",
        json!({"jsonrpc": "2.0", "id": 9, "method": "tools/list"})
    )
    .expect("write");
    let reply = reply_line(&mut reader);
    assert_eq!(
        reply["result"]["tools"].as_array().expect("tools").len(),
        enclave::mcp::proto::definitions().len()
    );
}
