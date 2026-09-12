//! Typed client for the enclaved daemon's local socket.
//!
//! The daemon owns the machine's enclave membership; anything on the machine
//! that wants to know who is reachable, or to drop a file on someone's
//! computer, asks it over a unix socket at
//! `$XDG_RUNTIME_DIR/enclave/enclaved.sock`.
//!
//! The protocol is one JSON line in, one JSON line out, per connection: send
//! `{"op":"status"}` and read the answer, then the connection is done. Every
//! reply carries `ok`; a false one carries `error`, which comes back here as
//! [`Error::Daemon`] with the daemon's own words.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};

/// Where the daemon listens.
pub fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(Path::new(&dir).join("enclave/enclaved.sock"))
}

// ----- replies --------------------------------------------------------------

/// What this machine is signed in as, and the enclaves it belongs to.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Status {
    /// The signed-in account, absent when nobody is signed in.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub enclaves: Vec<Enclave>,
}

/// One enclave this machine is on. `up` is whether its link is currently
/// carrying traffic; the daemon's remaining per-enclave fields are kept
/// verbatim in `extra` until they are worth a name here.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Enclave {
    #[serde(default)]
    pub up: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One computer on one of this machine's enclaves.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Computer {
    #[serde(rename = "Machine", default)]
    pub machine: String,
    #[serde(rename = "Email", default)]
    pub email: String,
    #[serde(rename = "Enclave", default)]
    pub enclave: String,
    #[serde(rename = "CompanyID", default, deserialize_with = "text_or_number")]
    pub company_id: String,
    #[serde(rename = "Online", default)]
    pub online: bool,
    #[serde(rename = "IPs", default, deserialize_with = "null_as_empty")]
    pub ips: Vec<String>,
    /// This machine itself.
    #[serde(rename = "Self", default)]
    pub is_self: bool,
}

/// Go marshals an empty slice as `null`; read that as no addresses.
fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

/// The daemon numbers its companies, and says so as a JSON number. An id is an
/// identifier either way, so take whichever it sends and keep it as text.
fn text_or_number<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}

// ----- errors ---------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    /// No `XDG_RUNTIME_DIR`, so there is no socket path to try.
    NoSocket,
    /// The socket would not talk: not running, no permission, cut short.
    Io(std::io::Error),
    /// A reply that is not the shape this protocol promises.
    Protocol(String),
    /// The daemon said no, in its own words.
    Daemon(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoSocket => write!(f, "XDG_RUNTIME_DIR is not set"),
            Error::Io(e) => write!(f, "enclaved socket: {e}"),
            Error::Protocol(m) => write!(f, "enclaved replied with {m}"),
            Error::Daemon(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

// ----- calls ----------------------------------------------------------------

/// This machine's sign-in and enclaves.
pub fn status() -> Result<Status> {
    parse_status(&ask(&serde_json::json!({ "op": "status" }))?)
}

/// Every computer this machine can see, across all its enclaves.
pub fn peers() -> Result<Vec<Computer>> {
    parse_peers(&ask(&serde_json::json!({ "op": "peers" }))?)
}

/// Sends a file to `addr` — a computer name, `@person`, or `@person/computer`.
/// `path` must be absolute. Returns the daemon's confirmation message.
pub fn send(addr: &str, path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .ok_or_else(|| Error::Protocol("a path that is not UTF-8".to_string()))?;
    parse_send(&ask(
        &serde_json::json!({ "op": "send", "computer": addr, "path": path }),
    )?)
}

/// One request line in, one response line out, then the socket is done.
fn ask(request: &serde_json::Value) -> Result<String> {
    let path = socket_path().ok_or(Error::NoSocket)?;
    let mut sock = UnixStream::connect(&path)?;
    let mut line = serde_json::to_string(request).map_err(|e| Error::Protocol(e.to_string()))?;
    line.push('\n');
    sock.write_all(line.as_bytes())?;
    sock.flush()?;
    let mut reply = String::new();
    BufReader::new(sock).read_line(&mut reply)?;
    Ok(reply)
}

// ----- reply parsing --------------------------------------------------------

/// The `ok` / `error` envelope every reply wears, unwrapped to its body.
fn envelope(line: &str) -> Result<serde_json::Value> {
    let line = line.trim();
    if line.is_empty() {
        return Err(Error::Protocol("nothing".to_string()));
    }
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| Error::Protocol(format!("{e}")))?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let message = value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("enclaved refused without saying why");
        return Err(Error::Daemon(message.to_string()));
    }
    Ok(value)
}

fn parse_status(line: &str) -> Result<Status> {
    serde_json::from_value(envelope(line)?).map_err(|e| Error::Protocol(e.to_string()))
}

fn parse_peers(line: &str) -> Result<Vec<Computer>> {
    let value = envelope(line)?;
    let computers = value
        .get("computers")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if computers.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(computers).map_err(|e| Error::Protocol(e.to_string()))
}

fn parse_send(line: &str) -> Result<String> {
    Ok(envelope(line)?
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_status_reply() {
        let line = r#"{"ok":true,"email":"ed@example.com","enclaves":[{"name":"acme","up":true},{"name":"side","up":false}]}"#;
        let status = parse_status(line).expect("status");
        assert_eq!(status.email.as_deref(), Some("ed@example.com"));
        assert_eq!(status.enclaves.len(), 2);
        assert!(status.enclaves[0].up);
        assert!(!status.enclaves[1].up);
        assert_eq!(
            status.enclaves[0].extra.get("name").and_then(|v| v.as_str()),
            Some("acme")
        );
    }

    #[test]
    fn reads_a_peers_reply() {
        let line = concat!(
            r#"{"ok":true,"computers":["#,
            r#"{"Machine":"ed-mac","Email":"ed@example.com","Enclave":"acme","#,
            r#""CompanyID":"c1","Online":true,"IPs":["100.64.0.2"],"Self":false},"#,
            r#"{"Machine":"omarchy","Email":"ed@example.com","Enclave":"acme","#,
            r#""CompanyID":"c1","Online":false,"IPs":null,"Self":true}]}"#,
        );
        let computers = parse_peers(line).expect("peers");
        assert_eq!(computers.len(), 2);
        assert_eq!(computers[0].machine, "ed-mac");
        assert_eq!(computers[0].ips, ["100.64.0.2"]);
        assert!(computers[0].online);
        assert!(!computers[0].is_self);
        // A nil slice arrives as null, not as [].
        assert!(computers[1].ips.is_empty());
        assert!(computers[1].is_self);
    }

    /// What the daemon actually sends: the company id as a number.
    #[test]
    fn a_numeric_company_id_is_still_read() {
        let line = concat!(
            r#"{"ok":true,"computers":[{"Machine":"omarchy","Email":"ed@omarchy","#,
            r#""Enclave":"Eduard Moldovan AB","CompanyID":1,"Online":true,"#,
            r#""IPs":["100.64.0.1"],"Self":true}]}"#,
        );
        let computers = parse_peers(line).expect("peers");
        assert_eq!(computers[0].company_id, "1");
    }

    #[test]
    fn reads_a_send_reply() {
        let ok = r#"{"ok":true,"message":"sent report.pdf to ed-mac"}"#;
        assert_eq!(parse_send(ok).unwrap(), "sent report.pdf to ed-mac");
    }

    #[test]
    fn a_refusal_carries_the_daemons_words() {
        let line = r#"{"ok":false,"error":"no computer named \"nope\""}"#;
        match parse_send(line) {
            Err(Error::Daemon(m)) => assert_eq!(m, "no computer named \"nope\""),
            other => panic!("expected the daemon's error, got {other:?}"),
        }
        assert!(matches!(parse_status(""), Err(Error::Protocol(_))));
        assert!(matches!(parse_peers("not json"), Err(Error::Protocol(_))));
    }

    #[test]
    fn missing_computers_is_an_empty_list() {
        assert!(parse_peers(r#"{"ok":true}"#).unwrap().is_empty());
        assert!(parse_peers(r#"{"ok":true,"computers":null}"#).unwrap().is_empty());
    }
}
