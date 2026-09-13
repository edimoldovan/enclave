//! Update loop, input handling and command dispatch.
//!
//! Drawing lives in the `ui` modules, each of which adds methods to `PostApp`
//! (defined in `state.rs`). What is here is the frame: step the webview's own
//! loop, take whatever a second command line handed over, absorb the answers
//! that came back from the network, gather commands from the keymap and the
//! ribbon, run them, then draw.

use eframe::egui::{Context, ViewportCommand};

use crate::client::Job;
use crate::commands::Command;
use crate::nav::View;
use crate::state::PostApp;
use crate::web;

/// How far one arrow key moves a message body, in points. About three lines —
/// far enough to be reading, near enough to keep your place.
const BODY_STEP: f32 = 60.0;

/// How far PageDown and PageUp go: ten rows of a list, and the same ten steps
/// of a message body — a screenful either way.
const PAGE_STEP: i32 = 10;

/// One frame, for the loop that runs WebKit while a message is open.
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

impl eframe::App for PostApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        if self.wake.is_none() {
            self.wake = Some(ctx.clone());
        }
        // Restyle in place when the Omarchy theme changes under us. The body
        // pane is a web page, so it only takes the new colors when it is next
        // built — which is the next message opened.
        if let Some(palette) = self.theme_watcher.changed(ctx) {
            self.palette = palette;
            ctx.set_visuals(enclave_ui::theme::visuals(&palette));
        }
        // WebKit does its work on GTK's loop, which nothing else is stepping.
        web::pump();

        self.take_handoffs(ctx);
        for done in self.jobs.drain() {
            self.absorb(done);
        }
        // Whatever view we ended up on, a body belongs only to a message.
        if self.nav.top().message().is_none() {
            self.web.hide();
        }

        let mut cmds = self.collect_input(ctx);
        cmds.extend(self.pane_input(ctx));
        cmds.extend(self.ribbon(ctx));
        self.status_bar(ctx);
        cmds.extend(self.view(ctx, frame));

        for cmd in cmds {
            self.exec(cmd, ctx);
        }

        self.shortcuts_window(ctx);
        self.update_title(ctx);
        self.ensure_loaded();
        // A call in flight repaints when it lands; keep the window ticking so
        // the pending line does not sit there after the answer arrives.
        if self.jobs.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // A body on screen is WebKit's, and WebKit only runs in `web::pump`
        // above: a frame is what draws the message, scrolls it and delivers
        // its keys, so while one is up the frames keep coming.
        if self.web.placed() != web::Placed::Nowhere {
            ctx.request_repaint_after(FRAME);
        }
    }
}

impl PostApp {
    /// Views handed over by a second `enclave post …`: go there, and come to
    /// the front, because someone typed a command and is waiting to see it.
    fn take_handoffs(&mut self, ctx: &Context) {
        let mut arrived = Vec::new();
        if let Some(rx) = &self.open_rx {
            while let Ok(view) = rx.try_recv() {
                arrived.push(view);
            }
        }
        let Some(view) = arrived.pop() else {
            return;
        };
        self.goto(view);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// The commands the keyboard asked for this frame.
    fn collect_input(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        // While a text field or the shortcut viewer has the keyboard, the
        // window's own chords stay out of the way.
        if self.shortcuts || ctx.wants_keyboard_input() {
            return cmds;
        }
        ctx.input_mut(|input| {
            for (shortcut, cmd) in &self.keymap.bindings {
                if input.consume_shortcut(shortcut) {
                    cmds.push(*cmd);
                }
            }
        });
        cmds
    }

    /// The commands the body pane handed back this frame.
    ///
    /// Over that pane the keyboard is WebKit's — a native view with the focus
    /// — so the chords the window claims arrive from GTK, not from egui. The
    /// pane sends the chord and the keymap says what it means, which is the
    /// same answer the window would have given had it seen the key itself.
    fn pane_input(&mut self, ctx: &Context) -> Vec<Command> {
        let mut cmds = Vec::new();
        for pressed in self.web.pressed() {
            let bound = self.keymap.bindings.iter().find(|(key, _)| *key == pressed);
            if let Some((_, cmd)) = bound {
                cmds.push(*cmd);
            }
        }
        if !cmds.is_empty() {
            // The command runs after this frame has drawn, so the window owes
            // itself one more.
            ctx.request_repaint();
        }
        cmds
    }

    /// Runs one command.
    pub fn exec(&mut self, cmd: Command, ctx: &Context) {
        match cmd {
            Command::Back => self.back(),
            Command::Accounts => self.goto(View::Accounts),
            Command::Refresh => self.refresh(),
            Command::Open => self.open_focused(),
            Command::Next => self.step(1),
            Command::Previous => self.step(-1),
            Command::PageNext => self.step(PAGE_STEP),
            Command::PagePrevious => self.step(-PAGE_STEP),
            Command::Last => self.to_end(),
            Command::Delete => self.delete_current(),
            Command::MarkUnread => self.mark_unread(),
            Command::AddAccount => {
                if !self.connecting {
                    self.start(Job::AddAccount);
                }
            }
            Command::ShortcutHelp => self.shortcuts = !self.shortcuts,
            Command::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
        }
    }

    /// Asks the provider again for whatever is on screen.
    fn refresh(&mut self) {
        match self.nav.top().clone() {
            View::Accounts => self.accounts = None,
            View::Inbox { .. } => {
                self.mailbox = None;
                self.paging = false;
            }
            View::Detail { .. } => {
                self.message = None;
                self.body = None;
                self.web.hide();
            }
        }
        self.asked = None;
    }

    /// Opens the row the keyboard is on.
    fn open_focused(&mut self) {
        if self.nav.top() == &View::Accounts {
            self.activate_account();
            return;
        }
        let (Some(account), Some(row)) = (
            self.account().map(str::to_owned),
            self.rows().get(self.focus).cloned(),
        ) else {
            return;
        };
        self.open(&account, &row.id);
    }

    /// Enter on the account list: the selected account's mail, or the add
    /// flow when the keyboard is on the stop after the last account.
    fn activate_account(&mut self) {
        let selected = self
            .accounts
            .as_ref()
            .and_then(|list| list.get(self.account_focus))
            .cloned();
        match selected {
            Some(account) => self.goto(View::Inbox { account }),
            None if !self.connecting => self.start(Job::AddAccount),
            None => {}
        }
    }

    /// Opens one message. Reading it is what marks it read.
    pub fn open(&mut self, account: &str, id: &str) {
        self.goto(View::Detail {
            account: account.to_string(),
            id: id.to_string(),
        });
    }

    /// Down and Up: the next row in a list, a notch of the body in a message.
    /// The same two keys, because in both views they mean "further on".
    fn step(&mut self, by: i32) {
        if self.nav.top().message().is_some() {
            self.body_scroll += by as f32 * BODY_STEP;
        } else if self.nav.top() == &View::Accounts {
            self.move_account_focus(by);
        } else {
            self.move_focus(by);
        }
    }

    /// Moves the keyboard's stop in the account list, and asks it to bring
    /// the stop it landed on back into view.
    pub fn move_account_focus(&mut self, by: i32) {
        let last = self.account_stops() as i32 - 1;
        let next = (self.account_focus as i32 + by).clamp(0, last) as usize;
        if next == self.account_focus {
            return;
        }
        self.account_focus = next;
        self.follow_focus = true;
    }

    /// Moves the keyboard's row, and asks the list to bring it back into view.
    ///
    /// Landing on the last row loaded — or pressing on past it — is the ask
    /// for the page after this one, which goes on the end.
    pub fn move_focus(&mut self, by: i32) {
        let rows = self.rows().len();
        if rows == 0 {
            return;
        }
        let next = (self.focus as i32 + by).clamp(0, rows as i32 - 1) as usize;
        if by > 0 && next == rows - 1 {
            self.load_more();
        }
        if next == self.focus {
            return;
        }
        self.focus = next;
        self.follow_focus = true;
    }

    /// End: the last row loaded, and the page after it. In a message the key
    /// is the body's, not the list's.
    fn to_end(&mut self) {
        if self.nav.top().message().is_some() {
            return;
        }
        if self.nav.top() == &View::Accounts {
            self.move_account_focus(self.account_stops() as i32);
            return;
        }
        let rows = self.rows().len();
        if rows == 0 {
            return;
        }
        if self.focus != rows - 1 {
            self.focus = rows - 1;
            self.follow_focus = true;
        }
        self.load_more();
    }

    /// Trashes the message on screen, or the row the keyboard is on.
    fn delete_current(&mut self) {
        let Some(account) = self.account().map(str::to_owned) else {
            return;
        };
        let id = match self.nav.top().message() {
            Some(id) => Some(id.to_string()),
            None => self.rows().get(self.focus).map(|row| row.id.clone()),
        };
        if let Some(id) = id {
            self.delete(&account, &id);
        }
    }

    /// Trashes one message, named. The row buttons come through here.
    pub fn delete(&mut self, account: &str, id: &str) {
        self.start(Job::Delete {
            account: account.to_string(),
            id: id.to_string(),
        });
    }

    /// Puts the unread flag back on the message on screen.
    fn mark_unread(&mut self) {
        let (Some(account), Some(id)) = (
            self.account().map(str::to_owned),
            self.nav.top().message().map(str::to_owned),
        ) else {
            return;
        };
        self.start(Job::Mark {
            account,
            id,
            read: false,
        });
    }

    /// Starts whatever the view on screen still needs. One job per view: a
    /// refusal is said once, not retried every frame.
    fn ensure_loaded(&mut self) {
        if self.jobs.busy() {
            return;
        }
        let view = self.nav.top().clone();
        if self.asked.as_ref() == Some(&view) {
            return;
        }
        let job = match &view {
            View::Accounts => self.accounts.is_none().then_some(Job::Accounts),
            View::Inbox { account } => match &self.mailbox {
                Some(mailbox) if &mailbox.account == account => None,
                _ => Some(Job::List {
                    account: account.clone(),
                    page: None,
                }),
            },
            View::Detail { account, id } => match &self.message {
                Some(message) if &message.id == id => None,
                _ => Some(Job::Read {
                    account: account.clone(),
                    id: id.clone(),
                }),
            },
        };
        self.asked = Some(view);
        if let Some(job) = job {
            self.start(job);
        }
    }

    /// The title bar says where we are, and what is open.
    fn update_title(&mut self, ctx: &Context) {
        let title = match self.nav.top() {
            View::Accounts => "Post".to_string(),
            View::Inbox { account } => format!("Post — {account}"),
            View::Detail { account, .. } => match &self.message {
                Some(message) if !message.subject.is_empty() => {
                    format!("Post — {}", message.subject)
                }
                _ => format!("Post — {account}"),
            },
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Done, Mailbox};
    use serde_json::json;

    /// A mailbox of `rows` messages — more than fit on a screen, which is the
    /// only interesting case for scrolling.
    fn listing(rows: usize) -> Mailbox {
        let messages: Vec<_> = (0..rows)
            .map(|i| {
                json!({
                    "id": format!("18f{i}"),
                    "from": "Ale <ale@acme.com>",
                    "subject": "Re: the quote",
                    "date": "2025-09-12 10:33",
                    "unread": true
                })
            })
            .collect();
        Mailbox::of_answer(
            "ed@acme.com",
            &json!({"account": "ed@acme.com", "messages": messages}),
        )
    }

    fn inbox(rows: usize) -> PostApp {
        let mut app = PostApp::new(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        app.absorb(Done::List(listing(rows)));
        app
    }

    /// Down and Up move the selection, and ask the list to bring the row they
    /// landed on back into view. The ends of the list are ends: the selection
    /// stays, and nothing is asked of the list.
    #[test]
    fn the_arrows_move_the_selection_and_the_list_follows_it() {
        let mut app = inbox(40);
        assert_eq!(app.focus, 0);
        assert!(!app.follow_focus, "a fresh list is where it should be");

        app.move_focus(1);
        assert_eq!(app.focus, 1);
        assert!(app.follow_focus, "the list follows the selection");

        // The frame that scrolls takes the flag with it, so a mouse scrolling
        // away afterwards is left where the reader put it.
        app.follow_focus = false;
        app.move_focus(1);
        assert_eq!(app.focus, 2);
        assert!(app.follow_focus);

        app.focus = 0;
        app.follow_focus = false;
        app.move_focus(-1);
        assert_eq!(app.focus, 0, "the top is the top");
        assert!(!app.follow_focus, "nothing moved, so nothing follows");

        app.focus = 39;
        app.move_focus(1);
        assert_eq!(app.focus, 39, "and the bottom is the bottom");
        assert!(!app.follow_focus);
    }

    /// An empty list has nothing to select and nothing to scroll.
    #[test]
    fn the_arrows_do_nothing_to_an_empty_list() {
        let mut app = inbox(0);
        app.move_focus(1);
        assert_eq!(app.focus, 0);
        assert!(!app.follow_focus);
    }

    /// In a message the same two keys move the body instead: nothing is
    /// selected in a message, and the selection in the list behind it stays
    /// exactly where the reader left it.
    #[test]
    fn in_a_message_the_arrows_move_the_body() {
        let mut app = inbox(40);
        app.focus = 7;
        app.open("ed@acme.com", "18f7");

        app.step(1);
        assert!(app.body_scroll > 0.0, "down scrolls down");
        assert_eq!(app.focus, 7, "the list behind it did not move");

        let down = app.body_scroll;
        app.step(-1);
        assert_eq!(app.body_scroll, 0.0, "up undoes down");
        app.step(-1);
        assert_eq!(app.body_scroll, -down, "and keeps going up");
        assert!(!app.follow_focus);

        // The frame that scrolls takes it; the next one starts from nothing.
        let taken = std::mem::take(&mut app.body_scroll);
        assert!(taken != 0.0);
        assert_eq!(app.body_scroll, 0.0);

        // Back in the list, they move the selection again.
        app.back();
        app.step(1);
        assert_eq!(app.focus, 8);
        assert_eq!(app.body_scroll, 0.0);
    }
}
