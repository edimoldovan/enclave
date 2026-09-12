//! `enclave --serve`: the one process that is always there.
//!
//! It owns the app socket — binding it *is* the single-instance lock — and
//! three things that must not be duplicated: the MCP server an assistant talks
//! to, the registry of which process is showing which product, and the
//! confirmation broker. It shows no product itself, so nothing here knows what
//! a spreadsheet is; a `grido_` call is forwarded to whichever process holds
//! that role, and one is started if none does.
//!
//! It is headless: no event loop, no viewport, no window, ever, and it runs on
//! a machine with no display at all. The one thing it has to put on screen —
//! the question before something acts — is a process of its own,
//! `enclave --confirm`, one per question. That is why: a window is a whole
//! winit event loop for the life of the process, and the server would be paying
//! for it around the clock to show a dialog for a few seconds a day — on
//! Wayland paying with an empty window it has no way to unmap.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::confirm::Broker;
use crate::ipc::{self, Msg};
use crate::mcp::proto;

/// How long a tool call waits for a product window to answer. Generous: the
/// window may be asking the user something of its own.
const HOST_CALL_WAIT: Duration = Duration::from_secs(300);

/// How long the server waits for a window it just started to claim its role.
const HOST_START_WAIT: Duration = Duration::from_secs(30);

/// The only product with a window so far.
const GRIDO: &str = "grido";

/// One connected process that is showing a product.
struct Host {
    out: Mutex<UnixStream>,
    waiting: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
}

impl Host {
    fn new(out: UnixStream) -> Host {
        Host {
            out: Mutex::new(out),
            waiting: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Sends one request and waits for the reply carrying its id.
    fn request(&self, make: impl FnOnce(u64) -> Msg) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (answer, reply) = channel();
        self.waiting
            .lock()
            .map_err(|_| "the window registry is poisoned".to_string())?
            .insert(id, answer);

        let sent = self
            .out
            .lock()
            .map_err(|_| "the window registry is poisoned".to_string())
            .and_then(|mut out| ipc::send(&mut *out, &make(id)).map_err(|e| e.to_string()));
        if let Err(e) = sent {
            self.forget(id);
            return Err(format!("could not reach the window: {e}"));
        }

        match reply.recv_timeout(HOST_CALL_WAIT) {
            Ok(result) => result,
            Err(_) => {
                self.forget(id);
                Err("the window did not answer".to_string())
            }
        }
    }

    fn settle(&self, id: u64, result: Result<Value, String>) {
        if let Ok(mut waiting) = self.waiting.lock() {
            if let Some(answer) = waiting.remove(&id) {
                let _ = answer.send(result);
            }
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.remove(&id);
        }
    }

    /// The window went away: everything still waiting on it fails now rather
    /// than sitting there until its timeout.
    fn fail_all(&self, why: &str) {
        if let Ok(mut waiting) = self.waiting.lock() {
            for (_, answer) in waiting.drain() {
                let _ = answer.send(Err(why.to_string()));
            }
        }
    }
}

/// The server's shared state.
pub struct Server {
    pub broker: Arc<Broker>,
    hosts: Mutex<HashMap<String, Arc<Host>>>,
    arrived: Condvar,
}

impl Server {
    pub fn new(broker: Broker) -> Server {
        Server {
            broker: Arc::new(broker),
            hosts: Mutex::new(HashMap::new()),
            arrived: Condvar::new(),
        }
    }

    /// Runs one tool. Every call in the product goes through here, so this is
    /// where the confirmation gate sits — before the enclave acts and before a
    /// window is asked to do anything.
    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        self.broker.decide(name, &args)?;
        if name.starts_with(crate::mcp::tools::PREFIX) {
            return crate::mcp::tools::call(name, &args);
        }
        if name.starts_with(grido::mcp::tools::PREFIX) {
            let host = self.ensure_host(GRIDO)?;
            let tool = name.to_string();
            return host.request(move |id| Msg::Call { id, tool, args });
        }
        Err(format!("no such tool: {name}"))
    }

    /// Puts a file on screen in the product's window, starting one if needed.
    /// This is the second `enclave file.xlsx` of the day: one window, the file
    /// it was asked for, in front.
    pub fn open(&self, path: Option<&std::path::Path>) -> Result<Value, String> {
        let host = self.ensure_host(GRIDO)?;
        let path = path.map(|p| p.to_path_buf());
        host.request(move |id| Msg::Open {
            id: Some(id),
            path,
        })
    }

    /// Claims the role for a product, or says who has it.
    fn register(&self, product: &str, out: UnixStream) -> Result<Arc<Host>, String> {
        let mut hosts = self
            .hosts
            .lock()
            .map_err(|_| "the window registry is poisoned".to_string())?;
        if hosts.contains_key(product) {
            return Err(format!("a {product} window is already running"));
        }
        let host = Arc::new(Host::new(out));
        hosts.insert(product.to_string(), host.clone());
        self.arrived.notify_all();
        Ok(host)
    }

    /// Gives up the role, but only if the caller is still the one holding it.
    fn unregister(&self, product: &str, host: &Arc<Host>) {
        if let Ok(mut hosts) = self.hosts.lock() {
            if hosts.get(product).is_some_and(|h| Arc::ptr_eq(h, host)) {
                hosts.remove(product);
            }
        }
    }

    fn host(&self, product: &str) -> Option<Arc<Host>> {
        self.hosts.lock().ok()?.get(product).cloned()
    }

    /// The window for a product, started if there is none.
    fn ensure_host(&self, product: &str) -> Result<Arc<Host>, String> {
        if let Some(host) = self.host(product) {
            return Ok(host);
        }
        ipc::spawn_product(product);
        self.wait_for_host(product, HOST_START_WAIT)
    }

    fn wait_for_host(&self, product: &str, wait: Duration) -> Result<Arc<Host>, String> {
        let mut hosts = self
            .hosts
            .lock()
            .map_err(|_| "the window registry is poisoned".to_string())?;
        let deadline = std::time::Instant::now() + wait;
        loop {
            if let Some(host) = hosts.get(product) {
                return Ok(host.clone());
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(format!("the {product} window did not start"));
            }
            let (guard, _) = self
                .arrived
                .wait_timeout(hosts, left)
                .map_err(|_| "the window registry is poisoned".to_string())?;
            hosts = guard;
        }
    }
}

/// Binds the socket and serves until the process ends.
///
/// If another server already answers, this one has nothing to do and says so
/// by exiting quietly: whoever started it only wanted *a* server running.
pub fn run() {
    let path = proto::socket_path();
    let Some(listener) = proto::bind_socket(&path) else {
        return;
    };
    let server = Arc::new(Server::new(Broker::new(crate::confirm::allowlist_path())));

    // The thread that asks the questions: it starts a dialog process for each
    // one and waits for its answer, so nothing on this socket can approve.
    {
        let broker = server.broker.clone();
        std::thread::Builder::new()
            .name("enclave-confirm".into())
            .spawn(move || crate::confirm::serve_dialogs(&broker))
            .ok();
    }

    // Tell the assistants on this computer how to reach us, and teach them how
    // to use what they find. The server owns this because it owns the socket.
    std::thread::spawn(|| {
        grido::mcp::register::register_on_startup();
        let _ = grido::mcp::skill::install();
    });

    accept_loop(&server, listener);
    let _ = std::fs::remove_file(&path);
}

fn accept_loop(server: &Arc<Server>, listener: UnixListener) {
    for stream in listener.incoming().flatten() {
        let server = server.clone();
        std::thread::spawn(move || {
            if let Err(e) = serve_connection(&server, stream) {
                eprintln!("enclave: connection ended: {e}");
            }
        });
    }
}

/// One connection, whichever kind it turns out to be.
///
/// MCP clients speak JSON-RPC; windows speak the `op` messages. Both arrive
/// here, and a line's shape says which it is — so the shim keeps working
/// unchanged while a window on the same socket claims its role.
///
/// Public because the tests drive a real server through it over a socket pair,
/// which is every part of this file except the window.
pub fn serve_connection(server: &Arc<Server>, stream: UnixStream) -> std::io::Result<()> {
    let out = Arc::new(Mutex::new(stream.try_clone()?));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let reply = |msg: &Msg| -> std::io::Result<()> {
        let mut out = out
            .lock()
            .map_err(|_| std::io::Error::other("the connection is poisoned"))?;
        ipc::send(&mut *out, msg)
    };
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }
        if ipc::is_op(&line) {
            match Msg::parse(&line) {
                Some(Msg::Host { product }) => {
                    let mine = out
                        .lock()
                        .map_err(|_| std::io::Error::other("the connection is poisoned"))?
                        .try_clone()?;
                    match server.register(&product, mine) {
                        Ok(host) => {
                            reply(&Msg::Reply {
                                id: None,
                                result: Ok(json!("registered")),
                            })?;
                            // From here this connection belongs to that window.
                            read_host(server, &product, &host, reader);
                            return Ok(());
                        }
                        Err(e) => reply(&Msg::Reply {
                            id: None,
                            result: Err(e),
                        })?,
                    }
                }
                Some(Msg::Open { id, path }) => {
                    let result = server.open(path.as_deref());
                    reply(&Msg::Reply { id, result })?;
                }
                _ => reply(&Msg::Reply {
                    id: None,
                    result: Err("not something the server answers".to_string()),
                })?,
            }
            continue;
        }
        // MCP, one message per thread. A call can sit in the confirm queue for
        // two minutes; the next message on the same connection must not wait
        // behind it. Replies carry their request's id, so order is not ours to
        // keep.
        let server = server.clone();
        let out = out.clone();
        let message = line.clone();
        std::thread::spawn(move || {
            let dispatch = move |name: &str, args: Value| server.call_tool(name, args);
            if let Some(response) = proto::handle_message_with(&message, &dispatch) {
                if let Ok(mut out) = out.lock() {
                    let _ = writeln!(out, "{response}");
                    let _ = out.flush();
                }
            }
        });
    }
}

/// Reads a registered window's replies until it goes away.
fn read_host(
    server: &Arc<Server>,
    product: &str,
    host: &Arc<Host>,
    mut reader: BufReader<UnixStream>,
) {
    loop {
        match ipc::recv(&mut reader) {
            Ok(Some(Msg::Reply {
                id: Some(id),
                result,
            })) => host.settle(id, result),
            // A window says nothing else; ignore rather than drop the link.
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    server.unregister(product, host);
    host.fail_all("the window closed");
}
