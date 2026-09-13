//! The enclave's own tools: which enclaves this computer is on, who it can
//! reach, and dropping a file on one of them.
//!
//! They are answered by the enclaved daemon over its local socket, so they
//! need no window and no UI thread — `enclave mcp` serves them itself, and a
//! client that only ever asks who is reachable never opens anything.
//!
//! The mail verbs are served here too, at the bottom of this file: Post is a
//! library over files on disk, with no window and no process of its own, so
//! there is nothing for the shim to forward a `post_` call to. `post_show` is
//! the exception that proves it — asking to *see* mail rather than be told
//! about it — and it is answered here as well: the window is opened by the
//! same handoff-or-start the command line uses, so no server is involved in
//! that either.

use post_ui::View;
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

/// What the mail verbs are called.
pub const POST_PREFIX: &str = post::PREFIX;

/// The one mail tool the palette does not own. Every other `post_` verb is a
/// file on disk answered in place; this one puts a window on screen, so it
/// lives on this side — with the processes and the sockets — and Post's
/// library stays a library.
pub const POST_SHOW: &str = "post_show";

/// Post's seven verbs off the palette table it shares with `enclave post`, and
/// the window they are read in.
pub fn post_definitions() -> Vec<Value> {
    let mut tools = post::verbs::definitions();
    tools.push(json!({
        "name": POST_SHOW,
        "description": "Open the user's Post window on screen at one view: their accounts, \
                        one account's mail, or one message. Use this when the user asks to \
                        SEE their mail — \"open my inbox\", \"show me that message\" — and \
                        when a window is already up it navigates rather than opening a \
                        second one. It shows a window and returns nothing to read: to read \
                        mail yourself, use post_list and post_read.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "view": {
                    "type": "string",
                    "enum": ["accounts", "inbox", "message"],
                    "description": "\"accounts\" for the connected accounts, \"inbox\" for one \
                                    account's mail, \"message\" for one message",
                },
                "account": {
                    "type": "string",
                    "description": "account email as listed by post_accounts — required for \
                                    \"inbox\" and \"message\"",
                },
                "id": {
                    "type": "string",
                    "description": "message id, verbatim from post_list — required for \
                                    \"message\"",
                },
            },
            "required": ["view"],
        }
    }));
    tools
}

/// Runs one `post_` tool, here in this process. All of v1 is free — reading
/// mail, a read flag, a message to the trash, a window — so none of it goes
/// near the confirmation broker.
pub fn post_call(tool: &str, args: &Value) -> Result<Value, String> {
    if tool == POST_SHOW {
        return post_show(args);
    }
    post::verbs::call(tool, args)
}

/// Puts the Post window on screen at a view.
///
/// The two moves are the ones a second `enclave post list ed@…` makes, through
/// the same code: [`crate::posthost::open`] hands the view to the window that
/// is up, or starts one. No command line is run to do it.
fn post_show(args: &Value) -> Result<Value, String> {
    let view = requested_view(args)?;
    crate::posthost::open(&view)?;
    Ok(json!(opened(&view)))
}

/// The view a call asks for, or what is missing from it.
fn requested_view(args: &Value) -> Result<View, String> {
    match str_arg(args, "view")?.trim() {
        "accounts" => Ok(View::Accounts),
        "inbox" => Ok(View::Inbox {
            account: for_view(args, "account", "inbox")?,
        }),
        "message" => Ok(View::Detail {
            account: for_view(args, "account", "message")?,
            id: for_view(args, "id", "message")?,
        }),
        other => Err(format!(
            "\"{other}\" is not a view — use \"accounts\", \"inbox\" or \"message\""
        )),
    }
}

/// An argument one view cannot do without, named together with the view that
/// wanted it. Trimmed, like the command line trims the same word.
fn for_view(args: &Value, key: &str, view: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_owned())
        .ok_or_else(|| format!("the \"{view}\" view needs \"{key}\""))
}

/// The one line that goes back: what is on screen now.
fn opened(view: &View) -> String {
    match view {
        View::Accounts => "opened accounts".to_string(),
        View::Inbox { account } => format!("opened inbox {account}"),
        View::Detail { account, id } => format!("opened message {id} in {account}"),
    }
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
