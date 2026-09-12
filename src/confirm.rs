//! The confirmation broker: nothing acts on the user's behalf unasked.
//!
//! A tool that only looks at something runs straight away. A tool that *does*
//! something — writes a cell, deletes rows, sends a file to a colleague — stops
//! here first. The call is parked, the server's confirm window shows one
//! sentence of what is about to happen, and the answer comes from a click in
//! that window and nowhere else: there is no approve message on any socket, so
//! nothing an assistant can say counts as consent.
//!
//! No answer within [`TIMEOUT`] is a refusal. "Always allow this tool" is the
//! one way the question stops being asked, and it is written down in
//! `~/.config/enclave/allowlist.toml` where it can be read and deleted.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a question waits before it counts as a refusal.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// What the model is told when the answer is no. Same words either way: a
/// refusal is a refusal whether it was clicked or waited out.
pub const DENIED: &str = "denied by the user";

/// Tools that do something rather than look at something.
///
/// Grido owns the list of verbs that change a workbook; an unknown `grido_`
/// name counts as acting, so a tool added later is asked about until someone
/// says otherwise.
pub fn acting(tool: &str) -> bool {
    if tool == "enclave_send_file" {
        return true;
    }
    if tool.starts_with(grido::mcp::tools::PREFIX) {
        return grido::app::mutates(tool);
    }
    false
}

/// The one sentence the confirm window shows.
pub fn summary(tool: &str, args: &Value) -> String {
    if tool == "enclave_send_file" {
        let file = args
            .get("path")
            .and_then(Value::as_str)
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string())
            })
            .unwrap_or_else(|| "a file".to_string());
        let computer = args
            .get("computer")
            .and_then(Value::as_str)
            .unwrap_or("another computer");
        return format!("Send {file} to {computer} over your enclave.");
    }
    if tool.starts_with(grido::mcp::tools::PREFIX) {
        let what = grido::app::describe(tool, args);
        return format!("In Grido: {what}.");
    }
    format!("Run {tool}.")
}

/// The arguments, shortened to something a person can glance at.
pub fn digest(args: &Value) -> String {
    let text = match args {
        Value::Object(map) if map.is_empty() => String::new(),
        other => other.to_string(),
    };
    if text.chars().count() > 160 {
        let short: String = text.chars().take(157).collect();
        format!("{short}…")
    } else {
        text
    }
}

/// The answer to one question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Deny,
}

/// A question waiting for an answer, as the confirm window sees it.
#[derive(Debug, Clone)]
pub struct Waiting {
    pub id: u64,
    pub tool: String,
    pub summary: String,
    pub digest: String,
    /// Seconds left before this counts as a refusal.
    pub left: u64,
}

struct Pending {
    id: u64,
    tool: String,
    summary: String,
    digest: String,
    deadline: Instant,
    answer: Sender<Decision>,
}

struct State {
    next_id: u64,
    queue: Vec<Pending>,
    allowed: BTreeSet<String>,
    file: PathBuf,
}

/// The broker itself. One per server process; shared by every socket thread.
pub struct Broker {
    state: Mutex<State>,
    timeout: Duration,
    /// A handle on the UI thread, so a question wakes the window that asks it.
    wake: Mutex<Option<eframe::egui::Context>>,
}

impl Broker {
    /// Loads the allowlist from `file` (missing is fine) and waits for work.
    pub fn new(file: PathBuf) -> Broker {
        Broker::with_timeout(file, TIMEOUT)
    }

    /// The same with a shorter fuse, which is how the tests watch one burn out.
    pub fn with_timeout(file: PathBuf, timeout: Duration) -> Broker {
        Broker {
            state: Mutex::new(State {
                next_id: 1,
                queue: Vec::new(),
                allowed: load(&file),
                file,
            }),
            timeout,
            wake: Mutex::new(None),
        }
    }

    /// Gives the broker a way to repaint the window that shows its questions.
    pub fn set_wake(&self, ctx: eframe::egui::Context) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(ctx);
        }
    }

    /// True if this tool may act without being asked about.
    pub fn allowed(&self, tool: &str) -> bool {
        self.state
            .lock()
            .map(|s| s.allowed.contains(tool))
            .unwrap_or(false)
    }

    /// The gate every tool call passes through.
    ///
    /// Returns immediately for a tool that only reads, and for one the user has
    /// already allowed for good. Otherwise it blocks until the window is
    /// answered, or until the fuse burns out — which is a refusal.
    pub fn decide(&self, tool: &str, args: &Value) -> Result<(), String> {
        if !acting(tool) {
            return Ok(());
        }
        let (answer, wait) = channel();
        let id = {
            let mut state = self.state.lock().map_err(|_| DENIED.to_string())?;
            if state.allowed.contains(tool) {
                return Ok(());
            }
            let id = state.next_id;
            state.next_id += 1;
            state.queue.push(Pending {
                id,
                tool: tool.to_string(),
                summary: summary(tool, args),
                digest: digest(args),
                deadline: Instant::now() + self.timeout,
                answer,
            });
            id
        };
        self.nudge();

        match wait.recv_timeout(self.timeout + Duration::from_secs(1)) {
            Ok(Decision::Approve) => Ok(()),
            Ok(Decision::Deny) => Err(DENIED.to_string()),
            Err(_) => {
                // Nobody was there. Drop the question so the window stops
                // showing it, and refuse.
                self.drop_pending(id);
                Err(format!(
                    "{DENIED} — no answer within {} s",
                    self.timeout.as_secs()
                ))
            }
        }
    }

    /// The oldest unanswered question, with its countdown. `None` when there is
    /// nothing to ask, which is when the confirm window stays away.
    pub fn front(&self) -> Option<Waiting> {
        let Ok(state) = self.state.lock() else {
            return None;
        };
        let now = Instant::now();
        state.queue.first().map(|p| Waiting {
            id: p.id,
            tool: p.tool.clone(),
            summary: p.summary.clone(),
            digest: p.digest.clone(),
            left: p.deadline.saturating_duration_since(now).as_secs(),
        })
    }

    /// How many questions are waiting. The window shows "and 2 more".
    pub fn waiting(&self) -> usize {
        self.state.lock().map(|s| s.queue.len()).unwrap_or(0)
    }

    /// Answers one question. `always` on an approval writes the tool into the
    /// allowlist, and lets through anything else already queued for it.
    pub fn resolve(&self, id: u64, decision: Decision, always: bool) {
        let mut send_now: Vec<(Sender<Decision>, Decision)> = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            let Some(at) = state.queue.iter().position(|p| p.id == id) else {
                return;
            };
            let pending = state.queue.remove(at);
            if always && decision == Decision::Approve {
                state.allowed.insert(pending.tool.clone());
                let file = state.file.clone();
                let allowed = state.allowed.clone();
                if let Err(e) = save(&file, &allowed) {
                    eprintln!("enclave: could not write {}: {e}", file.display());
                }
                // Same tool, same answer, for anything already in the queue.
                let tool = pending.tool.clone();
                let mut i = 0;
                while i < state.queue.len() {
                    if state.queue[i].tool == tool {
                        let other = state.queue.remove(i);
                        send_now.push((other.answer, Decision::Approve));
                    } else {
                        i += 1;
                    }
                }
            }
            send_now.push((pending.answer, decision));
        }
        for (answer, decision) in send_now {
            let _ = answer.send(decision);
        }
    }

    /// Refuses anything whose countdown has run out. The waiting thread gives
    /// up on its own too; this is what keeps the window honest about it.
    pub fn expire(&self) {
        let mut expired: Vec<Sender<Decision>> = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            let now = Instant::now();
            let mut i = 0;
            while i < state.queue.len() {
                if state.queue[i].deadline <= now {
                    expired.push(state.queue.remove(i).answer);
                } else {
                    i += 1;
                }
            }
        }
        for answer in expired {
            let _ = answer.send(Decision::Deny);
        }
    }

    fn drop_pending(&self, id: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.queue.retain(|p| p.id != id);
        }
    }

    fn nudge(&self) {
        if let Ok(slot) = self.wake.lock() {
            if let Some(ctx) = slot.as_ref() {
                ctx.request_repaint();
            }
        }
    }
}

/// Where the allowlist lives: `$XDG_CONFIG_HOME/enclave/allowlist.toml`, or
/// `~/.config/enclave/allowlist.toml` when that is not set.
pub fn allowlist_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("enclave").join("allowlist.toml")
}

/// Reads the allowlist. Anything unreadable is treated as an empty one: the
/// safe direction is to ask.
pub fn load(file: &Path) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return BTreeSet::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        eprintln!("enclave: {} is not valid TOML — ignoring it", file.display());
        return BTreeSet::new();
    };
    table
        .get("allow")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Writes the allowlist back, creating the directory if this is the first one.
pub fn save(file: &Path, allowed: &BTreeSet<String>) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = String::from(
        "# Tools Enclave may run without asking. Delete a line to be asked again.\nallow = [\n",
    );
    for tool in allowed {
        // Tool names are plain identifiers; quote them and be done.
        text.push_str(&format!("  \"{}\",\n", tool.replace('"', "")));
    }
    text.push_str("]\n");
    std::fs::write(file, text)
}
