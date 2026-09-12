//! End-to-end MCP test.
//!
//! Speaks newline-delimited JSON-RPC to the protocol layer exactly as a client
//! would. The socket/GUI half is exercised by hand; here we drive the message
//! handling, which is where the logic lives. The tool surface itself is
//! covered by grido's own test.

use serde_json::{json, Value};

#[test]
fn initialize_reports_the_protocol_and_server() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string(),
    )
    .expect("a reply");
    assert_eq!(reply["id"], 1);
    assert_eq!(
        reply["result"]["protocolVersion"],
        enclave::mcp::proto::PROTOCOL_VERSION
    );
    assert_eq!(reply["result"]["serverInfo"]["name"], "enclave");
    assert!(reply["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_carries_the_whole_product() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":7,"method":"tools/list"}).to_string(),
    )
    .expect("a reply");
    let listed = reply["result"]["tools"].as_array().expect("tools");
    assert_eq!(
        listed.len(),
        enclave::mcp::tools::definitions().len() + grido::mcp::tools::definitions().len()
    );
    let names: Vec<&str> = listed
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    // One server, so every tool says which product it belongs to.
    for name in &names {
        assert!(
            name.starts_with(enclave::mcp::tools::PREFIX)
                || name.starts_with(grido::mcp::tools::PREFIX),
            "{name} is not named product_verb"
        );
    }
    // The network's own tools, by the names the daemon's shim used.
    for expected in ["enclave_status", "enclave_computers", "enclave_send_file"] {
        assert!(names.contains(&expected), "{expected} is missing");
    }
    assert!(names.contains(&"grido_workbook_info"));
}

/// A name with no product prefix is refused outright — a typo must never reach
/// an app, because reaching for the app is what opens a window.
#[test]
fn an_unprefixed_tool_is_refused() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":11,"method":"tools/call",
                "params":{"name":"workbook_info","arguments":{}}})
        .to_string(),
    )
    .expect("a reply");
    assert_eq!(reply["result"]["isError"], Value::Bool(true));
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("a message");
    assert!(text.contains("no such tool"), "got {text}");
}

#[test]
fn unknown_methods_are_a_protocol_error() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":3,"method":"nope"}).to_string(),
    )
    .expect("a reply");
    assert_eq!(reply["error"]["code"], -32601);
}

/// Notifications (no id) must not produce a response line.
#[test]
fn notifications_are_not_answered() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
    );
    assert!(reply.is_none());
}

#[test]
fn garbage_input_does_not_panic() {
    assert!(enclave::mcp::proto::handle_message("not json at all").is_none());
    assert!(enclave::mcp::proto::handle_message("").is_none());
}

/// A tool call with no app behind it fails as an `isError` *result*, not a
/// protocol error — the model is meant to read it and adapt.
#[test]
fn tool_failures_come_back_as_results() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":9,"method":"tools/call",
                "params":{"name":"grido_workbook_info","arguments":{}}})
        .to_string(),
    )
    .expect("a reply");
    assert!(reply.get("error").is_none(), "must not be a protocol error");
    assert_eq!(reply["result"]["isError"], Value::Bool(true));
}

#[test]
fn the_socket_path_is_absolute_and_user_specific() {
    let path = enclave::mcp::proto::socket_path();
    assert!(path.is_absolute());
    assert!(path.to_string_lossy().contains("enclave"));
}

/// initialize must carry instructions: clients without skills still need to
/// know how to drive this server.
#[test]
fn initialize_carries_instructions() {
    let reply = enclave::mcp::proto::handle_message(
        &json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}).to_string(),
    )
    .expect("a reply");
    let text = reply["result"]["instructions"]
        .as_str()
        .expect("instructions");
    assert!(
        text.contains("grido_workbook_info"),
        "should name the first call"
    );
    assert!(
        text.contains("grido_table_append"),
        "should point at record tools"
    );
    assert!(
        text.contains("enclave_send_file"),
        "should name the network tools too"
    );
    assert!(text.len() > 200);
}
