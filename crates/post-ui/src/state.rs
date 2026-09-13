//! What the window is holding, and every way that changes.
//!
//! `PostApp` lives here; `app.rs` owns the update loop and command dispatch,
//! and each `ui::*` module adds its own `impl PostApp` block.
//!
//! Nothing in this file draws, and nothing in it touches the network: an answer
//! arrives as a [`Done`] and [`PostApp::absorb`] is the one place it turns into
//! what is on screen. That is why the window's behaviour — a message marked
//! read the moment it is opened, a trashed row leaving the list, Back landing
//! where it should — can be tested with no display and no mailbox.

use std::sync::mpsc::Receiver;

use eframe::egui::Context;
use enclave_ui::theme::{Palette, ThemeWatcher};

use crate::client::{Done, Job, Jobs, Mailbox, Message};
use crate::commands::Command;
use crate::keymap::Keymap;
use crate::nav::{Nav, View};
use crate::web;

pub struct PostApp {
    pub nav: Nav,
    pub keymap: Keymap,

    /// The connected accounts. `None` until they have been read once.
    pub accounts: Option<Vec<String>>,
    /// The page of mail on screen, for whichever account the view is about.
    pub mailbox: Option<Mailbox>,
    /// The message on screen.
    pub message: Option<Message>,
    /// The saved HTML body as a page, ready for the webview. `None` when the
    /// message is plain text, or when its HTML could not be read back.
    pub body: Option<String>,
    /// Which row the keyboard is on, in the inbox.
    pub focus: usize,
    /// Which stop the keyboard is on, in the account list: one per account,
    /// and "Add account" on the end.
    pub account_focus: usize,
    /// Set when the keyboard moved the focus, so the list scrolls the row it
    /// landed on back into view. Cleared by the frame that does it — a mouse
    /// scrolling away from the selection is left alone.
    pub follow_focus: bool,
    /// Points the message body still owes a scroll, from the arrow keys. In
    /// points, positive downwards; taken by the frame that applies it.
    pub body_scroll: f32,

    /// Calls in flight, and their answers.
    pub jobs: Jobs,
    /// The view a load has already been started for, so a failure is reported
    /// once rather than retried every frame.
    pub asked: Option<View>,
    /// True while the page after the one on screen is on the wire. The list
    /// shows a quiet line at its end and asks for nothing more until it lands.
    pub paging: bool,
    /// True while Google's consent screen is open in the browser.
    pub connecting: bool,

    pub status: String,
    /// The shortcut viewer.
    pub shortcuts: bool,

    /// Views handed over by a second `enclave post …`.
    pub open_rx: Option<Receiver<View>>,
    /// A repaint handle, so an answer from a thread lands on screen at once.
    pub wake: Option<Context>,

    pub palette: Palette,
    pub theme_watcher: ThemeWatcher,
    pub last_title: String,

    /// The body pane, and what the display server allowed.
    pub web: web::Body,
    pub ready: web::Ready,
}

impl PostApp {
    /// A window showing `view`, with nothing loaded yet.
    pub fn new(view: View) -> PostApp {
        let (keymap, warnings) = crate::keymap::load();
        let web = web::Body::new(pane_keys(&keymap));
        PostApp {
            nav: Nav::of(view),
            keymap,
            accounts: None,
            mailbox: None,
            message: None,
            body: None,
            focus: 0,
            account_focus: 0,
            follow_focus: false,
            body_scroll: 0.0,
            jobs: Jobs::new(),
            asked: None,
            paging: false,
            connecting: false,
            status: first_warning(&warnings),
            shortcuts: false,
            open_rx: None,
            wake: None,
            palette: enclave_ui::theme::load(),
            theme_watcher: ThemeWatcher::new(),
            last_title: String::new(),
            web,
            ready: web::Ready::default(),
        }
    }

    /// Records what the display server allowed, and says so once if it is less
    /// than a pane inside this window.
    pub fn with_ready(mut self, ready: web::Ready) -> PostApp {
        self.ready = ready;
        if let Some(note) = ready.note()
            && self.status.is_empty()
        {
            self.status = note.to_string();
        }
        self
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    /// The account the view is about, if it is about one.
    pub fn account(&self) -> Option<&str> {
        self.nav.top().account()
    }

    /// How many stops the account list has for the keyboard: one per account
    /// connected, and "Add account" on the end. Never zero.
    pub fn account_stops(&self) -> usize {
        self.accounts.as_ref().map_or(0, Vec::len) + 1
    }

    /// The rows on screen, empty when nothing is loaded.
    pub fn rows(&self) -> &[crate::client::Row] {
        self.mailbox.as_ref().map(|m| m.rows.as_slice()).unwrap_or(&[])
    }

    /// Starts a call, waking the window when it comes back.
    pub fn start(&mut self, job: Job) {
        self.set_status(job.about());
        if job == Job::AddAccount {
            self.connecting = true;
        }
        self.jobs.start(job, self.wake.clone());
    }

    /// Goes to a view and forgets whatever belonged to the last one, so the
    /// next frame loads what this one needs.
    pub fn goto(&mut self, view: View) {
        if self.nav.top() == &view {
            return;
        }
        let account_changed = self.nav.top().account() != view.account();
        let message_changed = self.nav.top().message() != view.message();
        self.nav.goto(view);
        if account_changed {
            self.mailbox = None;
            self.focus = 0;
            self.paging = false;
        }
        if message_changed {
            self.forget_message();
        }
    }

    /// Back one view, dropping what only the view we left was showing.
    pub fn back(&mut self) {
        let had = self.nav.top().message().map(str::to_owned);
        if !self.nav.back() {
            return;
        }
        if had.is_some() && self.nav.top().message().is_none() {
            self.forget_message();
        }
    }

    /// Asks for the page after the one on screen, to go on the end of it.
    ///
    /// Reaching the end of the list is the whole of the ask — the wheel near
    /// the bottom, or the selection landing on the last row. Nothing to do at
    /// the end of a mailbox, or while a page is already on its way.
    pub fn load_more(&mut self) {
        if self.paging {
            return;
        }
        let Some(mailbox) = &self.mailbox else {
            return;
        };
        let (account, Some(page)) = (mailbox.account.clone(), mailbox.next_page.clone()) else {
            return;
        };
        self.paging = true;
        self.start(Job::List {
            account,
            page: Some(page),
        });
    }

    /// Drops the open message and takes its body off screen.
    pub fn forget_message(&mut self) {
        self.message = None;
        self.body = None;
        self.web.hide();
        self.asked = None;
    }

    /// One answer, as the change it makes.
    ///
    /// Reading a message marks it read — the mail library does that as part of
    /// the read, and the row follows here — so opening a message is the whole
    /// of "mark read on open"; there is no second call to forget.
    pub fn absorb(&mut self, done: Done) {
        match done {
            Done::Accounts(list) => {
                self.accounts = Some(list);
            }
            Done::List(mailbox) => {
                // A page that refreshed itself can land after the reader has
                // moved on: it belongs to the account it names, and to no
                // other view.
                if self.nav.top().account().is_some_and(|a| a != mailbox.account) {
                    return;
                }
                // The same mailbox again — a cached page corrected — keeps the
                // selection where the reader put it.
                let same = self.mailbox.as_ref().is_some_and(|m| m.account == mailbox.account);
                self.focus = if same {
                    self.focus.min(mailbox.rows.len().saturating_sub(1))
                } else {
                    0
                };
                self.mailbox = Some(mailbox);
                self.status.clear();
            }
            Done::More(more) => {
                self.paging = false;
                self.status.clear();
                // The page that came back belongs to the mailbox that asked
                // for it, and to no other. The selection does not move: rows
                // only ever go on the end.
                if let Some(mailbox) = &mut self.mailbox
                    && mailbox.account == more.account
                {
                    mailbox.append(more);
                }
            }
            Done::Read(message) => {
                self.status.clear();
                if let Some(mailbox) = &mut self.mailbox {
                    mailbox.set_unread(&message.id, false);
                }
                // Back may have been pressed while this was on the wire. The
                // message was still read, and its row says so — but nobody is
                // dragged into a message they have already left.
                if self.nav.top().message() != Some(message.id.as_str()) {
                    return;
                }
                self.body = body_page(&message, &self.palette);
                self.message = Some(message);
            }
            Done::Marked { id, read } => {
                if let Some(mailbox) = &mut self.mailbox {
                    mailbox.set_unread(&id, !read);
                }
                self.set_status(if read {
                    format!("{id} marked read.")
                } else {
                    format!("{id} marked unread.")
                });
            }
            Done::Deleted { id } => {
                if let Some(mailbox) = &mut self.mailbox {
                    mailbox.remove(&id);
                }
                if self.nav.top().message() == Some(id.as_str()) {
                    self.back();
                }
                self.focus = self.focus.min(self.rows().len().saturating_sub(1));
                self.set_status(format!("{id} moved to the trash."));
            }
            Done::Added(email) => {
                self.connecting = false;
                let accounts = self.accounts.get_or_insert_with(Vec::new);
                if !accounts.contains(&email) {
                    accounts.push(email.clone());
                }
                self.set_status(format!("Connected {email}."));
            }
            Done::Failed { about, error } => {
                self.connecting = false;
                self.paging = false;
                self.set_status(format!("{about} {error}"));
            }
        }
    }
}

/// What a keymap complained about, in one line. A file written for another app
/// — `./keymap.toml` in a directory that is not Post's — objects to every
/// binding in it, and a status bar is not the place to read all of them.
fn first_warning(warnings: &[String]) -> String {
    match warnings.split_first() {
        None => String::new(),
        Some((first, [])) => first.clone(),
        Some((first, rest)) => format!("{first} (and {} more)", rest.len()),
    }
}

/// The chords the body pane hands back to the window instead of letting WebKit
/// keep them: the ones bound to a command that means the same thing over an
/// open message — leaving it, and trashing it. Read from the keymap so there is
/// one list of them and the reader's own file is it. Everything else stays
/// WebKit's, which is what keeps the arrows and PageUp/PageDown scrolling.
fn pane_keys(keymap: &Keymap) -> Vec<eframe::egui::KeyboardShortcut> {
    keymap
        .bindings
        .iter()
        .filter(|(_, cmd)| matches!(cmd, Command::Back | Command::Delete))
        .map(|(chord, _)| *chord)
        .collect()
}

/// The message's saved HTML as a page to render, or nothing when there is no
/// HTML to read — in which case the detail view shows the text body.
fn body_page(message: &Message, palette: &Palette) -> Option<String> {
    let path = message.html_path.as_deref()?;
    let html = std::fs::read_to_string(path).ok()?;
    let rgb = |c: eframe::egui::Color32| [c.r(), c.g(), c.b()];
    Some(web::page(
        &html,
        rgb(palette.bg),
        rgb(palette.fg),
        rgb(palette.accent),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Mailbox, Message};
    use serde_json::json;

    fn listing() -> Mailbox {
        Mailbox::of_answer(
            "ed@acme.com",
            &json!({
                "account": "ed@acme.com",
                "messages": [
                    {"id": "18f3", "from": "Ale <ale@acme.com>", "subject": "Re: the quote",
                     "date": "2025-09-12 10:33", "unread": true},
                    {"id": "18f2", "from": "EY <cristiano@ey.com>", "subject": "Slides",
                     "date": "2025-09-11 08:02", "unread": true}
                ]
            }),
        )
    }

    fn message(id: &str) -> Message {
        Message {
            id: id.to_string(),
            account: "ed@acme.com".to_string(),
            from: "Ale <ale@acme.com>".to_string(),
            subject: "Re: the quote".to_string(),
            date: "2025-09-12 10:33".to_string(),
            text: "Looks good — send it.".to_string(),
            html_path: None,
            attachments: Vec::new(),
        }
    }

    /// A page of `ids`, and where the mailbox continues after it.
    fn paged(ids: &[&str], next: Option<&str>) -> Mailbox {
        let messages: Vec<_> = ids
            .iter()
            .map(|id| {
                json!({"id": id, "from": "Ale <ale@acme.com>", "subject": "Re: the quote",
                       "date": "2025-09-12 10:33", "unread": true})
            })
            .collect();
        Mailbox::of_answer(
            "ed@acme.com",
            &json!({"account": "ed@acme.com", "messages": messages, "next_page": next}),
        )
    }

    fn inbox() -> PostApp {
        let mut app = PostApp::new(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        app.absorb(Done::List(listing()));
        app
    }

    /// A chord as the keymap spells it.
    fn chord(text: &str) -> eframe::egui::KeyboardShortcut {
        enclave_ui::keymap::parse_chord(text).expect("a chord")
    }

    /// A keymap built for one test: chord and command, as a `keymap.toml` line
    /// would name them.
    fn keymap_of(bindings: &[(&str, Command)]) -> Keymap {
        Keymap {
            bindings: bindings.iter().map(|(key, cmd)| (chord(key), *cmd)).collect(),
            source: "a test".to_string(),
        }
    }

    /// The keymap Post ships with, out of its own compiled-in file — the same
    /// bindings the loader would find, with no file on this computer involved.
    fn shipped_keymap() -> Keymap {
        let bindings = crate::keymap::DEFAULT_KEYMAP
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .filter_map(|(key, id)| {
                let key = enclave_ui::keymap::parse_chord(key.trim().trim_matches('"'))?;
                Some((key, Command::from_id(id.trim().trim_matches('"'))?))
            })
            .collect();
        Keymap {
            bindings,
            source: "built-in".to_string(),
        }
    }

    /// A row tapped, and the answer back: the whole of opening a message.
    fn opened(id: &str) -> PostApp {
        let mut app = inbox();
        app.open("ed@acme.com", id);
        app.absorb(Done::Read(message(id)));
        app
    }

    /// Opening a message reads it, and a read message is no longer bold.
    #[test]
    fn opening_a_message_marks_its_row_read() {
        let mut app = inbox();
        assert!(app.mailbox.as_ref().expect("a page").row("18f3").expect("a row").unread);
        assert_eq!(app.mailbox.as_ref().expect("a page").unread(), 2);

        app.open("ed@acme.com", "18f3");
        app.absorb(Done::Read(message("18f3")));

        let mailbox = app.mailbox.as_ref().expect("a page");
        assert!(!mailbox.row("18f3").expect("a row").unread, "the row is read now");
        assert!(mailbox.row("18f2").expect("a row").unread, "and only that row");
        assert_eq!(mailbox.unread(), 1);
        // The message is on screen, with the inbox behind it.
        assert_eq!(
            app.nav.top(),
            &View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string()
            }
        );
        assert_eq!(app.nav.depth(), 2, "the window opened at the inbox");
        assert_eq!(app.message.as_ref().expect("a message").id, "18f3");
    }

    /// A reader who goes back before the message arrives stays where they are —
    /// and the message is still read, because reading it is what marked it.
    #[test]
    fn an_answer_that_arrives_too_late_does_not_drag_anyone_back() {
        let mut app = inbox();
        app.open("ed@acme.com", "18f3");
        app.back();
        app.absorb(Done::Read(message("18f3")));

        assert_eq!(
            app.nav.top(),
            &View::Inbox {
                account: "ed@acme.com".to_string()
            }
        );
        assert!(app.message.is_none(), "nothing was forced open");
        assert!(
            !app.mailbox.as_ref().expect("a page").row("18f3").expect("a row").unread,
            "it was read all the same"
        );
    }

    /// Marking it unread again puts the weight back.
    #[test]
    fn a_message_can_be_made_unread_again() {
        let mut app = opened("18f3");
        app.absorb(Done::Marked {
            id: "18f3".to_string(),
            read: false,
        });
        assert!(app.mailbox.as_ref().expect("a page").row("18f3").expect("a row").unread);
        assert!(app.status.contains("marked unread"), "got {}", app.status);

        app.absorb(Done::Marked {
            id: "18f3".to_string(),
            read: true,
        });
        assert!(!app.mailbox.as_ref().expect("a page").row("18f3").expect("a row").unread);
    }

    /// Trashing the message on screen drops its row and goes back to the list —
    /// never leaving a detail view of something that is gone.
    #[test]
    fn trashing_the_open_message_falls_back_to_the_list() {
        let mut app = opened("18f3");
        app.absorb(Done::Deleted {
            id: "18f3".to_string(),
        });

        assert_eq!(app.rows().len(), 1);
        assert!(app.mailbox.as_ref().expect("a page").row("18f3").is_none());
        assert_eq!(
            app.nav.top(),
            &View::Inbox {
                account: "ed@acme.com".to_string()
            }
        );
        assert!(app.message.is_none(), "nothing is left open");
        assert!(app.body.is_none());
        assert!(app.status.contains("trash"), "got {}", app.status);
    }

    /// Trashing a row from the list leaves the list where it is.
    #[test]
    fn trashing_a_row_from_the_list_stays_in_the_list() {
        let mut app = inbox();
        app.focus = 1;
        app.absorb(Done::Deleted {
            id: "18f2".to_string(),
        });
        assert_eq!(app.rows().len(), 1);
        assert_eq!(
            app.nav.top(),
            &View::Inbox {
                account: "ed@acme.com".to_string()
            }
        );
        assert_eq!(app.focus, 0, "the focus follows the shorter list");
    }

    /// Back from a message drops it and keeps the list; back from the view the
    /// window opened at does nothing, because nothing was walked through.
    #[test]
    fn back_drops_what_only_that_view_was_showing() {
        let mut app = opened("18f3");
        app.back();
        assert!(app.message.is_none());
        assert!(app.mailbox.is_some(), "the list is still loaded");
        let inbox = View::Inbox {
            account: "ed@acme.com".to_string(),
        };
        assert_eq!(app.nav.top(), &inbox);
        app.back();
        assert_eq!(app.nav.top(), &inbox, "the inbox it opened at is the floor");
    }

    /// Another account is another mailbox: the rows on screen do not follow it.
    #[test]
    fn changing_account_forgets_the_page_that_was_open() {
        let mut app = inbox();
        app.focus = 1;
        app.goto(View::Inbox {
            account: "ale@acme.com".to_string(),
        });
        assert!(app.mailbox.is_none());
        assert_eq!(app.focus, 0);
        assert_eq!(app.account(), Some("ale@acme.com"));
    }

    /// A connected account joins the list without a second read of the file,
    /// and the browser wait is over.
    #[test]
    fn a_connected_account_joins_the_list() {
        let mut app = PostApp::new(View::Accounts);
        app.absorb(Done::Accounts(vec!["ed@acme.com".to_string()]));
        app.connecting = true;
        app.absorb(Done::Added("ale@acme.com".to_string()));
        assert!(!app.connecting);
        assert_eq!(
            app.accounts.as_deref(),
            Some(["ed@acme.com".to_string(), "ale@acme.com".to_string()].as_slice())
        );
        // Connecting one that is already there does not double it up.
        app.absorb(Done::Added("ale@acme.com".to_string()));
        assert_eq!(app.accounts.as_ref().expect("a list").len(), 2);
    }

    /// A call that failed says so and stops waiting — it never leaves the
    /// window pretending something is still in flight.
    #[test]
    fn a_refusal_is_said_out_loud() {
        let mut app = PostApp::new(View::Accounts);
        app.connecting = true;
        app.absorb(Done::Failed {
            about: "Fetching ed@acme.com…".to_string(),
            error: "Gmail refused the call (403)".to_string(),
        });
        assert!(!app.connecting);
        assert!(app.status.contains("403"), "got {}", app.status);
        assert!(app.status.contains("Fetching"), "got {}", app.status);
    }

    /// A keymap written for another app objects to every line in it; the
    /// status bar says the first and how many followed, never all of them.
    #[test]
    fn a_pile_of_keymap_warnings_is_one_line() {
        assert_eq!(first_warning(&[]), "");
        assert_eq!(
            first_warning(&["keymap: unknown command \"move_up\"".to_string()]),
            "keymap: unknown command \"move_up\""
        );
        let many: Vec<String> = (0..40).map(|i| format!("warning {i}")).collect();
        let line = first_warning(&many);
        assert_eq!(line, "warning 0 (and 39 more)");
        assert!(!line.contains("warning 7"));
    }

    /// A window opened straight at a message — `enclave post read ed@… 18f3` —
    /// has nothing behind it: that view is the floor, so Back does nothing and
    /// the message it opened at stays open.
    #[test]
    fn a_window_opened_at_a_message_has_nothing_behind_it() {
        let detail = View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        };
        let mut app = PostApp::new(detail.clone());
        assert_eq!(app.nav.depth(), 1);
        app.absorb(Done::Read(message("18f3")));
        assert_eq!(app.message.as_ref().expect("a message").id, "18f3");

        app.back();
        assert_eq!(app.nav.top(), &detail, "there was nowhere to go");
        assert!(app.message.is_some(), "and nothing was dropped on the way");
    }

    // --------------------------------------------------------------- paging

    /// The page after the one on screen is asked for once: while it is on the
    /// wire the list asks for nothing more, and at the end of a mailbox — or
    /// with no mailbox at all — there is nothing to ask for.
    #[test]
    fn a_page_is_asked_for_once_and_only_where_there_is_one() {
        let mut app = inbox();
        app.absorb(Done::List(paged(&["18f3", "18f2"], Some("tok3n"))));

        app.load_more();
        assert!(app.paging, "a page is on its way");
        assert_eq!(
            app.status,
            Job::List {
                account: "ed@acme.com".to_string(),
                page: Some("tok3n".to_string()),
            }
            .about()
        );

        // A second ask while that one is in flight starts nothing: the status
        // a started job sets is not set again.
        app.set_status("");
        app.load_more();
        assert_eq!(app.status, "", "nothing was started");
        assert!(app.paging);
    }

    /// The end of a mailbox has no token, so there is no page to ask for — and
    /// asking leaves the list where it is rather than waiting forever.
    #[test]
    fn there_is_nothing_to_page_at_the_end_of_a_mailbox() {
        let mut app = inbox();
        assert_eq!(app.mailbox.as_ref().expect("a page").next_page, None);
        app.set_status("");
        app.load_more();
        assert!(!app.paging);
        assert_eq!(app.status, "");

        // And a window with nothing loaded yet has nothing to page either.
        let mut app = PostApp::new(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        app.set_status("");
        app.load_more();
        assert!(!app.paging);
        assert_eq!(app.status, "");
    }

    /// The page that comes back goes on the end, and the reader's place in the
    /// list does not move: rows only ever arrive below them. A row that is
    /// somehow in both pages — the boundary shifted under a mailbox that took
    /// a new message meanwhile — is in the list once.
    #[test]
    fn the_page_after_goes_on_the_end_and_the_selection_stays() {
        let mut app = inbox();
        app.absorb(Done::List(paged(&["18f3", "18f2"], Some("tok3n"))));
        app.focus = 1;
        app.paging = true;
        app.set_status("Fetching more…");

        app.absorb(Done::More(paged(&["18f2", "18f1", "18f0"], None)));

        let ids: Vec<&str> = app.rows().iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["18f3", "18f2", "18f1", "18f0"], "and nothing twice");
        assert_eq!(app.focus, 1, "the reader stayed where they were");
        assert!(!app.follow_focus, "so there is nothing to scroll to");
        assert!(!app.paging, "and the list may ask again");
        assert_eq!(app.status, "");
        assert_eq!(
            app.mailbox.as_ref().expect("a page").next_page,
            None,
            "that was the last page"
        );
    }

    /// A page belongs to the mailbox that asked for it, and to no other: a
    /// reader who changed account while it was on the wire is left alone.
    #[test]
    fn a_page_for_another_mailbox_is_not_appended() {
        let mut app = inbox();
        app.paging = true;
        let mut elsewhere = paged(&["aaa", "bbb"], None);
        elsewhere.account = "ale@acme.com".to_string();

        app.absorb(Done::More(elsewhere));
        assert_eq!(app.rows().len(), 2, "this mailbox is as it was");
        assert!(app.mailbox.as_ref().expect("a page").row("aaa").is_none());
        assert!(!app.paging, "and the list is not left waiting");
    }

    /// A page that failed takes the line off the end of the list — the reader
    /// can ask again, rather than watching it wait forever.
    #[test]
    fn a_page_that_failed_stops_the_paging() {
        let mut app = inbox();
        app.paging = true;
        app.absorb(Done::Failed {
            about: "Fetching more…".to_string(),
            error: "Gmail refused the call (429)".to_string(),
        });
        assert!(!app.paging);
        assert!(app.status.contains("429"), "got {}", app.status);
    }

    // ------------------------------------------------------- the pane's keys

    /// The body pane hands the window back the chords bound to leaving a
    /// message and to trashing it, and keeps every other key for WebKit — which
    /// is what leaves the arrows and the page keys scrolling the body.
    #[test]
    fn the_pane_hands_back_exactly_the_keys_bound_to_back_and_delete() {
        let keymap = keymap_of(&[
            ("Escape", Command::Back),
            ("ArrowLeft", Command::Back),
            ("ArrowDown", Command::Next),
            ("PageDown", Command::PageNext),
            ("Delete", Command::Delete),
            ("Ctrl+R", Command::Refresh),
            ("Ctrl+U", Command::MarkUnread),
        ]);
        assert_eq!(
            pane_keys(&keymap),
            vec![chord("Escape"), chord("ArrowLeft"), chord("Delete")]
        );

        // The reader's own file is the list: rebind them and the pane follows,
        // with no second list of keys anywhere to fall out of step.
        let rebound = keymap_of(&[
            ("Ctrl+B", Command::Back),
            ("Escape", Command::Quit),
            ("Ctrl+K", Command::Delete),
            ("ArrowLeft", Command::Previous),
        ]);
        assert_eq!(pane_keys(&rebound), vec![chord("Ctrl+B"), chord("Ctrl+K")]);

        // A keymap that binds neither takes nothing from WebKit at all.
        assert!(pane_keys(&keymap_of(&[("ArrowDown", Command::Next)])).is_empty());
    }

    /// And the keymap Post ships with: the three ways out of a message and the
    /// one that trashes it, and nothing else taken from the page.
    #[test]
    fn the_shipped_keymap_gives_the_pane_back_the_keys_it_needs() {
        let keys = pane_keys(&shipped_keymap());
        for claimed in ["Escape", "Backspace", "ArrowLeft", "Delete"] {
            assert!(keys.contains(&chord(claimed)), "{claimed} is not handed back");
        }
        assert_eq!(keys.len(), 4, "and nothing else is taken from WebKit");
        for webkits in ["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End", "Ctrl+R"] {
            assert!(!keys.contains(&chord(webkits)), "{webkits} is WebKit's");
        }
    }

    /// With no accounts the window still has something to say.
    #[test]
    fn an_empty_account_list_is_a_state_of_its_own() {
        let mut app = PostApp::new(View::Accounts);
        assert_eq!(app.accounts, None, "nothing read yet");
        app.absorb(Done::Accounts(Vec::new()));
        assert_eq!(app.accounts.as_deref(), Some([].as_slice()));
        assert!(app.rows().is_empty());
    }
}
