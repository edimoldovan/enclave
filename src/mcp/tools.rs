//! The enclave's own tools: which enclaves this computer is on, who it can
//! reach, and dropping a file on one of them.
//!
//! They are answered by the enclaved daemon over its local socket, so they
//! need no window and no UI thread — `enclave mcp` serves them itself, and a
//! client that only ever asks who is reachable never opens anything.

use serde_json::{json, Value};

/// What every tool here is called. The shim uses it to tell a windowless call
/// from one that belongs to an app.
pub const PREFIX: &str = "enclave_";

pub fn definitions() -> Vec<Value> {
    let tool = |name: &str, desc: &str, props: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": desc,
            "inputSchema": {
                "type": "object",
                "properties": props,
                "required": required,
            }
        })
    };
    vec![
        tool(
            "enclave_status",
            "This computer's enclave memberships: account email and each \
             enclave with its connection state.",
            json!({}),
            &[],
        ),
        tool(
            "enclave_computers",
            "The computers reachable on the user's enclaves: machine name, \
             owner email, enclave, online state.",
            json!({}),
            &[],
        ),
        tool(
            "enclave_send_file",
            "Send a local file to a colleague's computer over the enclave. It \
             lands in their Enclaved/<this computer>/ folder.",
            json!({
                "computer": {
                    "type": "string",
                    "description": "target machine name as listed by enclave_computers, \
                                    or @person, or @person/computer"
                },
                "path": {"type": "string", "description": "absolute path of the file to send"},
            }),
            &["computer", "path"],
        ),
    ]
}

/// Runs one `enclave_` tool.
///
/// Errors are strings: they go back to the model as an `isError` result, in
/// the daemon's own words.
pub fn call(tool: &str, args: &Value) -> Result<Value, String> {
    match tool {
        "enclave_status" => status(),
        "enclave_computers" => computers(),
        "enclave_send_file" => send_file(args),
        other => Err(format!("no such tool: {other}")),
    }
}

fn status() -> Result<Value, String> {
    let status = enclave_client::status().map_err(explain)?;
    let enclaves: Vec<Value> = status
        .enclaves
        .iter()
        .map(|e| {
            // Whatever the daemon named it by, plus the one field we read.
            let mut fields = e.extra.clone();
            fields.insert("up".to_string(), json!(e.up));
            Value::Object(fields)
        })
        .collect();
    Ok(json!({ "email": status.email, "enclaves": enclaves }))
}

fn computers() -> Result<Value, String> {
    let computers = enclave_client::peers().map_err(explain)?;
    Ok(json!({
        "computers": computers.iter().map(|c| json!({
            "machine": c.machine,
            "email": c.email,
            "enclave": c.enclave,
            "online": c.online,
            "self": c.is_self,
        })).collect::<Vec<_>>(),
    }))
}

fn send_file(args: &Value) -> Result<Value, String> {
    let computer = str_arg(args, "computer")?;
    let path = std::path::PathBuf::from(str_arg(args, "path")?);
    if !path.is_absolute() {
        return Err(format!(
            "\"{}\" is not an absolute path — pass the full path to the file",
            path.display()
        ));
    }
    let message = enclave_client::send(computer, &path).map_err(explain)?;
    Ok(json!({ "message": message }))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing \"{key}\""))
}

/// The daemon's own words, with the usual cause named: a socket that will not
/// answer means enclaved is not running on this computer.
fn explain(e: enclave_client::Error) -> String {
    match &e {
        enclave_client::Error::NoSocket | enclave_client::Error::Io(_) => {
            format!("enclaved is not running on this computer ({e})")
        }
        _ => e.to_string(),
    }
}
