//! The only file that calls the mail library.
//!
//! Everything above this line works in plain types — [`Mailbox`], [`Row`],
//! [`Message`] — and never sees a `serde_json::Value`, exactly as Grido's UI
//! never sees IronCalc. What the window runs are the same seven verbs the
//! assistant and the terminal run: one palette, one implementation, so a
//! message deleted from a row and a message deleted from a script are the same
//! call.
//!
//! Mail is on the network and the window is not allowed to stop for it, so a
//! job runs on a thread of its own and its answer comes back down a channel.
//! The parsing is separate from the running, which is what lets it be tested
//! with no account, no token and no network.

use std::sync::mpsc::{channel, Receiver, Sender};

use serde_json::{json, Value};

/// One message's row in a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub unread: bool,
}

/// One page of one account's mail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mailbox {
    pub account: String,
    pub rows: Vec<Row>,
    /// True when this page came off the disk rather than off the wire — the
    /// mail library hands the last one over at once and refreshes behind it,
    /// so a fresher page is on its way down [`Jobs::drain`].
    pub cached: bool,
    /// The token for the page after this one, when the mailbox has one. `None`
    /// is the end of the mailbox: there is nothing more to append.
    pub next_page: Option<String>,
}

impl Mailbox {
    /// A `post_list` answer as rows.
    pub fn of_answer(account: &str, answer: &Value) -> Mailbox {
        let rows = answer
            .get("messages")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().map(row_of).collect())
            .unwrap_or_default();
        Mailbox {
            account: answer
                .get("account")
                .and_then(Value::as_str)
                .unwrap_or(account)
                .to_string(),
            rows,
            cached: answer.get("cached").and_then(Value::as_bool) == Some(true),
            next_page: answer
                .get("next_page")
                .and_then(Value::as_str)
                .filter(|token| !token.trim().is_empty())
                .map(str::to_owned),
        }
    }

    pub fn row(&self, id: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.id == id)
    }

    /// Sets one row's unread flag. Nothing to do when the row is not in this
    /// page — a message read from a command line need not be listed here.
    pub fn set_unread(&mut self, id: &str, unread: bool) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.unread = unread;
        }
    }

    /// Drops one row, after it has gone to the trash.
    pub fn remove(&mut self, id: &str) {
        self.rows.retain(|row| row.id != id);
    }

    /// Adds the page after this one to the end of it, and takes on where the
    /// mailbox now continues. Rows already here are not added twice — a page
    /// boundary can shift under a mailbox that took a new message meanwhile.
    pub fn append(&mut self, more: Mailbox) {
        for row in more.rows {
            if !self.rows.iter().any(|have| have.id == row.id) {
                self.rows.push(row);
            }
        }
        self.next_page = more.next_page;
    }

    /// How many rows are still unread — the count on the account row.
    pub fn unread(&self) -> usize {
        self.rows.iter().filter(|row| row.unread).count()
    }
}

/// One message, as the detail view shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub account: String,
    pub from: String,
    pub subject: String,
    pub date: String,
    /// The body as text: what a plain-text message says, and what an HTML one
    /// says once it is stripped. Shown when there is no HTML to render.
    pub text: String,
    /// The saved HTML body, written by the mail library under the state dir.
    pub html_path: Option<String>,
    pub attachments: Vec<String>,
}

impl Message {
    /// A `post_read` answer as a message.
    pub fn of_answer(account: &str, answer: &Value) -> Message {
        let text = |key: &str| {
            answer
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Message {
            id: text("id"),
            account: account.to_string(),
            from: text("from"),
            subject: text("subject"),
            date: text("date"),
            text: text("text"),
            html_path: answer
                .get("html_path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_owned),
            attachments: answer
                .get("attachments")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|a| a.get("filename").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

fn row_of(row: &Value) -> Row {
    let text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Row {
        id: text("id"),
        from: text("from"),
        subject: text("subject"),
        date: text("date"),
        unread: row.get("unread").and_then(Value::as_bool) == Some(true),
    }
}

/// A page the mail library refreshed on its own, as the thing the window
/// keeps. A refresh that failed says so rather than leaving a cached page to
/// look like a fresh one.
fn refreshed(page: &Value) -> Done {
    match page.get("error").and_then(Value::as_str) {
        Some(error) => Done::Failed {
            about: "Refreshing the mailbox:".to_string(),
            error: error.to_string(),
        },
        None => Done::List(Mailbox::of_answer("", page)),
    }
}

/// Something to ask the mail library for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Accounts,
    /// One page of a mailbox. `page` is a token from the page before it, which
    /// makes this an append rather than the mailbox itself.
    List { account: String, page: Option<String> },
    Read { account: String, id: String },
    Mark { account: String, id: String, read: bool },
    Delete { account: String, id: String },
    AddAccount,
}

impl Job {
    /// What to say while this is in flight.
    pub fn about(&self) -> String {
        match self {
            Job::Accounts => "Reading the account list…".to_string(),
            Job::List { account, page } => match page {
                Some(_) => "Fetching more…".to_string(),
                None => format!("Fetching {account}…"),
            },
            Job::Read { .. } => "Opening the message…".to_string(),
            Job::Mark { .. } => "Marking…".to_string(),
            Job::Delete { .. } => "Moving to the trash…".to_string(),
            Job::AddAccount => {
                "Waiting for Google's sign-in in your browser…".to_string()
            }
        }
    }

    /// The verb and its arguments: the same call the assistant and the terminal
    /// make.
    fn verb(&self) -> (&'static str, Value) {
        match self {
            Job::Accounts => ("post_accounts", json!({})),
            Job::List { account, page } => match page {
                Some(page) => ("post_list", json!({"email": account, "page": page})),
                None => ("post_list", json!({ "email": account })),
            },
            Job::Read { account, id } => ("post_read", json!({"email": account, "id": id})),
            Job::Mark { account, id, read } => (
                "post_mark",
                json!({"email": account, "id": id, "read": read}),
            ),
            Job::Delete { account, id } => ("post_delete", json!({"email": account, "id": id})),
            Job::AddAccount => ("post_add_account", json!({})),
        }
    }

    /// Runs it. Blocking — this is what the thread is for.
    pub fn run(self) -> Done {
        let (tool, args) = self.verb();
        match post::verbs::call(tool, &args) {
            Ok(answer) => self.answered(&answer),
            Err(error) => Done::Failed {
                about: self.about(),
                error,
            },
        }
    }

    /// One verb's answer, as the thing the window keeps.
    fn answered(self, answer: &Value) -> Done {
        match self {
            Job::Accounts => Done::Accounts(
                answer
                    .get("accounts")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|a| a.get("email").and_then(Value::as_str))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            Job::List { account, page } => {
                let mailbox = Mailbox::of_answer(&account, answer);
                match page {
                    Some(_) => Done::More(mailbox),
                    None => Done::List(mailbox),
                }
            }
            Job::Read { account, .. } => Done::Read(Message::of_answer(&account, answer)),
            Job::Mark { id, read, .. } => Done::Marked { id, read },
            Job::Delete { id, .. } => Done::Deleted { id },
            Job::AddAccount => Done::Added(
                answer
                    .get("email")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        }
    }
}

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    Accounts(Vec<String>),
    List(Mailbox),
    /// The page after the one on screen, to go on the end of it.
    More(Mailbox),
    Read(Message),
    Marked { id: String, read: bool },
    Deleted { id: String },
    Added(String),
    Failed { about: String, error: String },
}

/// The jobs in flight, and the channel their answers come back on.
pub struct Jobs {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    /// Pages the mail library refreshed on its own, after handing over a
    /// cached one. Nobody asked for these, so they arrive on a channel of
    /// their own rather than as a job's answer.
    fresh: Receiver<Value>,
    running: usize,
    /// True between a cached page and the fresh one that follows it, so the
    /// window keeps ticking until the correction lands.
    awaiting: bool,
}

impl Default for Jobs {
    fn default() -> Jobs {
        Jobs::new()
    }
}

impl Jobs {
    pub fn new() -> Jobs {
        let (tx, rx) = channel();
        Jobs {
            tx,
            rx,
            // Subscribing is also what lets the mail library answer from its
            // cache: it only does that where something is listening for the
            // refresh, and this window is that something.
            fresh: post::gmail::subscribe(),
            running: 0,
            awaiting: false,
        }
    }

    /// True while anything is in flight — a job, or a refresh behind a cached
    /// page.
    pub fn busy(&self) -> bool {
        self.running > 0 || self.awaiting
    }

    /// Starts a job on a thread of its own. `wake` is the window's repaint
    /// handle, so an answer lands on screen without the mouse having to move.
    pub fn start(&mut self, job: Job, wake: Option<eframe::egui::Context>) {
        let tx = self.tx.clone();
        self.running += 1;
        let started = std::thread::Builder::new()
            .name("post-job".into())
            .spawn(move || {
                let done = job.run();
                let _ = tx.send(done);
                if let Some(ctx) = wake {
                    ctx.request_repaint();
                }
            });
        if started.is_err() {
            self.running -= 1;
            let _ = self.tx.send(Done::Failed {
                about: "Starting the call".to_string(),
                error: "this computer would not start another thread".to_string(),
            });
        }
    }

    /// Everything that has come back since the last frame: the jobs that
    /// finished, and then any page that refreshed itself behind a cached one.
    pub fn drain(&mut self) -> Vec<Done> {
        let mut done = Vec::new();
        while let Ok(answer) = self.rx.try_recv() {
            self.running = self.running.saturating_sub(1);
            if let Done::List(mailbox) = &answer
                && mailbox.cached
            {
                self.awaiting = true;
            }
            done.push(answer);
        }
        while let Ok(page) = self.fresh.try_recv() {
            self.awaiting = false;
            done.push(refreshed(&page));
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing exactly as `post_list` answers it.
    fn listing() -> Value {
        json!({
            "account": "ed@acme.com",
            "messages": [
                {"id": "18f3", "from": "Ale <ale@acme.com>", "subject": "Re: the quote",
                 "date": "2025-09-12 10:33", "unread": true},
                {"id": "18f2", "from": "EY <cristiano@ey.com>", "subject": "Slides",
                 "date": "2025-09-11 08:02", "unread": false}
            ],
            "next_page": "tok3n"
        })
    }

    #[test]
    fn a_listing_becomes_rows() {
        let mailbox = Mailbox::of_answer("ed@acme.com", &listing());
        assert_eq!(mailbox.account, "ed@acme.com");
        assert_eq!(mailbox.rows.len(), 2);
        assert_eq!(mailbox.rows[0].subject, "Re: the quote");
        assert_eq!(mailbox.rows[0].date, "2025-09-12 10:33");
        assert!(mailbox.rows[0].unread);
        assert!(!mailbox.rows[1].unread);
        assert_eq!(mailbox.unread(), 1);
        assert_eq!(mailbox.next_page.as_deref(), Some("tok3n"));

        // An empty mailbox is an answer, not a failure.
        let empty = Mailbox::of_answer("ed@acme.com", &json!({"messages": []}));
        assert!(empty.rows.is_empty());
        assert_eq!(empty.account, "ed@acme.com");
        assert_eq!(empty.next_page, None, "the end of a mailbox has no token");
    }

    /// The page after this one goes on the end, selection and all — and a row
    /// that is somehow in both pages is still only in the list once.
    #[test]
    fn a_later_page_goes_on_the_end() {
        let mut mailbox = Mailbox::of_answer("ed@acme.com", &listing());
        let more = Mailbox::of_answer(
            "ed@acme.com",
            &json!({
                "account": "ed@acme.com",
                "messages": [
                    {"id": "18f2", "from": "EY <cristiano@ey.com>", "subject": "Slides",
                     "date": "2025-09-11 08:02", "unread": false},
                    {"id": "18f1", "from": "Fredrik <fredrik@engelbrektsloppet.se>",
                     "subject": "Vinterveckan", "date": "2025-09-10 19:20", "unread": true}
                ]
            }),
        );
        mailbox.append(more);
        let ids: Vec<&str> = mailbox.rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["18f3", "18f2", "18f1"]);
        assert_eq!(mailbox.next_page, None, "and that was the last page");
    }

    /// The unread flag is the whole of "bold": rows carry it, and marking a
    /// message read clears it in place.
    #[test]
    fn marking_a_row_read_clears_its_weight() {
        let mut mailbox = Mailbox::of_answer("ed@acme.com", &listing());
        assert!(mailbox.row("18f3").expect("a row").unread);

        mailbox.set_unread("18f3", false);
        assert!(!mailbox.row("18f3").expect("a row").unread);
        assert_eq!(mailbox.unread(), 0);

        mailbox.set_unread("18f3", true);
        assert!(mailbox.row("18f3").expect("a row").unread);

        // A message this page has never heard of changes nothing.
        mailbox.set_unread("nope", false);
        assert_eq!(mailbox.rows.len(), 2);
    }

    #[test]
    fn a_trashed_row_leaves_the_page() {
        let mut mailbox = Mailbox::of_answer("ed@acme.com", &listing());
        mailbox.remove("18f2");
        assert_eq!(mailbox.rows.len(), 1);
        assert!(mailbox.row("18f2").is_none());
        mailbox.remove("18f2");
        assert_eq!(mailbox.rows.len(), 1, "removing it twice is not an error");
    }

    #[test]
    fn a_read_message_carries_its_body_and_its_html() {
        let answer = json!({
            "id": "18f3",
            "from": "Ale <ale@acme.com>",
            "date": "2025-09-12 10:33",
            "subject": "Re: the quote",
            "text": "Looks good — send it.",
            "html_path": "/home/ed/.local/share/enclave/postbodies/ed@acme.com/18f3.html",
            "unread": false,
            "marked_read": true,
            "attachments": [{"filename": "quote.pdf", "mime": "application/pdf", "bytes": 8412}]
        });
        let message = Message::of_answer("ed@acme.com", &answer);
        assert_eq!(message.id, "18f3");
        assert_eq!(message.account, "ed@acme.com");
        assert_eq!(message.subject, "Re: the quote");
        assert_eq!(message.text, "Looks good — send it.");
        assert_eq!(message.attachments, vec!["quote.pdf".to_string()]);
        assert!(message.html_path.is_some());

        // A plain-text message has no HTML, and says so rather than pointing at
        // a file that is not there.
        let plain = Message::of_answer(
            "ed@acme.com",
            &json!({"id": "18f2", "text": "hi", "html_path": null}),
        );
        assert_eq!(plain.html_path, None);
        assert!(plain.attachments.is_empty());
    }

    /// Every job maps onto a verb from the palette, by its MCP name — the
    /// window adds no mail surface of its own.
    #[test]
    fn every_job_is_one_of_the_palettes_verbs() {
        let jobs = [
            Job::Accounts,
            Job::List {
                account: "ed@acme.com".to_string(),
                page: None,
            },
            Job::List {
                account: "ed@acme.com".to_string(),
                page: Some("tok3n".to_string()),
            },
            Job::Read {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string(),
            },
            Job::Mark {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string(),
                read: false,
            },
            Job::Delete {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string(),
            },
            Job::AddAccount,
        ];
        for job in jobs {
            let (tool, args) = job.verb();
            assert!(
                post::verbs::find(tool).is_some(),
                "{tool} is not on the palette"
            );
            assert!(!job.about().is_empty());
            // The arguments are the ones the verb asks for, checked by the
            // palette itself rather than by anything here.
            if tool != "post_accounts" && tool != "post_add_account" {
                assert!(args.get("email").is_some(), "{tool} lost its account");
            }
        }
    }

    /// The answers become state without a network anywhere near them.
    #[test]
    fn an_answer_becomes_what_the_window_keeps() {
        let accounts = Job::Accounts.answered(&json!({
            "accounts": [{"email": "ed@acme.com"}, {"email": "ale@acme.com"}]
        }));
        assert_eq!(
            accounts,
            Done::Accounts(vec!["ed@acme.com".to_string(), "ale@acme.com".to_string()])
        );

        let listed = Job::List {
            account: "ed@acme.com".to_string(),
            page: None,
        }
        .answered(&listing());
        assert!(matches!(listed, Done::List(mailbox) if mailbox.rows.len() == 2));

        // The same answer asked for with a token is the next page, not the
        // mailbox: it goes on the end rather than replacing what is there.
        let more = Job::List {
            account: "ed@acme.com".to_string(),
            page: Some("tok3n".to_string()),
        }
        .answered(&listing());
        assert!(matches!(more, Done::More(mailbox) if mailbox.rows.len() == 2));

        let marked = Job::Mark {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
            read: false,
        }
        .answered(&json!({"id": "18f3", "read": false}));
        assert_eq!(
            marked,
            Done::Marked {
                id: "18f3".to_string(),
                read: false
            }
        );

        let deleted = Job::Delete {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        }
        .answered(&json!({"id": "18f3", "trashed": true}));
        assert_eq!(
            deleted,
            Done::Deleted {
                id: "18f3".to_string()
            }
        );

        let added = Job::AddAccount.answered(&json!({"email": "ed@acme.com", "connected": true}));
        assert_eq!(added, Done::Added("ed@acme.com".to_string()));
    }

    /// A job with nothing to do still comes back, so the window never waits
    /// forever on a thread that has already finished.
    #[test]
    fn a_finished_job_comes_back_down_the_channel() {
        let mut jobs = Jobs::new();
        assert!(!jobs.busy());
        // post_accounts reads a file and touches no network.
        jobs.start(Job::Accounts, None);
        assert!(jobs.busy());
        let mut answers = Vec::new();
        for _ in 0..200 {
            answers.extend(jobs.drain());
            if !answers.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(answers.len(), 1, "the answer came back");
        assert!(matches!(answers[0], Done::Accounts(_)));
        assert!(!jobs.busy());
    }
}
