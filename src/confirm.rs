//! The confirmation broker: nothing acts on the user's behalf unasked.
//!
//! A tool that only looks at something runs straight away. A tool that *does*
//! something — writes a cell, deletes rows, sends a file to a colleague — stops
//! here first. The call is parked, a dialog shows one sentence of what is about
//! to happen, and the answer comes from a click in that dialog and nowhere
//! else: there is no approve message on any socket, so nothing an assistant can
//! say counts as consent.
//!
//! The server has no window of its own, so the dialog is its own short-lived
//! process — `enclave --confirm`, one per question, started by
//! [`serve_dialogs`]. It gets the question on its stdin and answers on its
//! stdout, and that pipe is the only channel an approval travels on. A child
//! that crashes, hangs or says nothing is a refusal.
//!
//! No answer within [`TIMEOUT`] is a refusal — counted down in the dialog, and
//! again here as a fuse, because the two are not the same process. "Always
//! allow this tool" is the one way the question stops being asked, and it is
//! written down in `~/.config/enclave/allowlist.toml` where it can be read and
//! deleted.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How long a question waits before it counts as a refusal.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// How often the asking thread looks again with nothing to ask: short enough
/// that a fuse burning out behind the current dialog is noticed.
const POLL: Duration = Duration::from_millis(250);

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

/// The one sentence the confirm dialog shows.
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
    /// Signalled when a question is queued, so the thread that asks them wakes
    /// without polling for it.
    asked: Condvar,
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
            asked: Condvar::new(),
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
    /// already allowed for good. Otherwise it blocks until the dialog is
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
    /// nothing to ask, which is when no dialog is on screen.
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

    /// How many questions are waiting.
    pub fn waiting(&self) -> usize {
        self.state.lock().map(|s| s.queue.len()).unwrap_or(0)
    }

    /// Blocks until there is a question to ask, refusing on the way anything
    /// whose fuse burned out while it queued behind another dialog.
    pub fn next_question(&self) -> Waiting {
        loop {
            self.expire();
            if let Some(question) = self.front() {
                return question;
            }
            match self.state.lock() {
                Ok(state) => {
                    let _ = self.asked.wait_timeout(state, POLL);
                }
                Err(_) => std::thread::sleep(POLL),
            }
        }
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
    /// up on its own too; this is the fuse, and it burns whether or not a
    /// dialog ever came up.
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
        self.asked.notify_all();
    }
}

/// One question as the dialog process receives it: everything it has to draw,
/// and nothing else. It knows no ids and holds no handle on the broker, so the
/// only thing it can do is answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub tool: String,
    /// The one sentence: what is about to happen.
    pub sentence: String,
    /// The arguments, shortened; empty when there are none worth showing.
    pub args: String,
    /// What the "always allow" checkbox says.
    pub always_label: String,
    /// Seconds on the countdown before the dialog refuses by itself.
    pub seconds: u64,
}

impl Request {
    /// The question at the front of the queue, as the dialog will see it.
    pub fn about(question: &Waiting) -> Request {
        Request {
            tool: question.tool.clone(),
            sentence: question.summary.clone(),
            args: question.digest.clone(),
            always_label: format!("Always allow {}", question.tool),
            seconds: question.left,
        }
    }

    /// The single line this travels as, on the child's stdin.
    pub fn encode(&self) -> String {
        json!({
            "tool": self.tool,
            "sentence": self.sentence,
            "args": self.args,
            "always_label": self.always_label,
            "seconds": self.seconds,
        })
        .to_string()
    }

    pub fn parse(line: &str) -> Option<Request> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let tool = value.get("tool")?.as_str()?.to_string();
        let always_label = match value.get("always_label").and_then(Value::as_str) {
            Some(label) => label.to_string(),
            None => format!("Always allow {tool}"),
        };
        Some(Request {
            tool,
            sentence: text("sentence"),
            args: text("args"),
            always_label,
            seconds: value
                .get("seconds")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| TIMEOUT.as_secs()),
        })
    }
}

/// What the dialog says back, on its stdout. Anything else — a crash, silence,
/// a line that will not parse — is [`Answer::deny`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub decision: Decision,
    pub always_allow: bool,
}

impl Answer {
    pub fn deny() -> Answer {
        Answer {
            decision: Decision::Deny,
            always_allow: false,
        }
    }

    pub fn encode(&self) -> String {
        json!({
            "decision": match self.decision {
                Decision::Approve => "approve",
                Decision::Deny => "deny",
            },
            "always_allow": self.always_allow,
        })
        .to_string()
    }

    pub fn parse(line: &str) -> Option<Answer> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        let decision = match value.get("decision")?.as_str()? {
            "approve" => Decision::Approve,
            "deny" => Decision::Deny,
            _ => return None,
        };
        Some(Answer {
            decision,
            always_allow: value
                .get("always_allow")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// Asks the queue's questions, one dialog at a time, for as long as the server
/// runs. The next child starts when the last one has answered, so there is
/// never a second dialog on screen competing for the same click.
pub fn serve_dialogs(broker: &Broker) {
    loop {
        let question = broker.next_question();
        let answer = ask(&Request::about(&question));
        broker.resolve(question.id, answer.decision, answer.always_allow);
    }
}

/// Runs one dialog process and waits for its one line.
///
/// The child is killed on the way out whatever it said, so a dialog never
/// outlives the question it was asked, and nothing is left running.
pub fn ask(request: &Request) -> Answer {
    let mut child = match dialog_command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("enclave: could not ask the user ({e}) — refusing");
            return Answer::deny();
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{}", request.encode());
        let _ = stdin.flush();
        // Dropping it closes the pipe: the dialog has all it will ever get.
    }

    // Read on a thread, so a child that never answers is a timeout here rather
    // than a server that waits for it forever.
    let (answered, line) = channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut first = String::new();
            let _ = BufReader::new(stdout).read_line(&mut first);
            let _ = answered.send(first);
        });
    }
    let answer = line
        .recv_timeout(Duration::from_secs(request.seconds) + Duration::from_secs(1))
        .ok()
        .and_then(|line| Answer::parse(&line));

    let _ = child.kill();
    let _ = child.wait();
    answer.unwrap_or_else(Answer::deny)
}

/// How the dialog is started: this same executable, in its dialog role.
///
/// `ENCLAVE_CONFIRM_CMD` replaces it, which is how the tests stand a script in
/// for a window.
fn dialog_command() -> Command {
    if let Some(spec) = std::env::var_os("ENCLAVE_CONFIRM_CMD") {
        let words: Vec<String> = spec
            .to_string_lossy()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if let Some((program, args)) = words.split_first() {
            let mut command = Command::new(program);
            command.args(args);
            return command;
        }
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("enclave"));
    let mut command = Command::new(exe);
    command.arg("--confirm");
    command
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
