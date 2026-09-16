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
            // The mouse's own back/forward buttons.
            if input.pointer.button_pressed(eframe::egui::PointerButton::Extra1) {
                cmds.push(Command::Back);
            }
            if input.pointer.button_pressed(eframe::egui::PointerButton::Extra2) {
                cmds.push(Command::Open);
            }
            // Two-finger swipe right = back, the way browsers read it: the
            // horizontal scroll a view with nothing to scroll sideways
            // receives, accumulated until it is unmistakably a swipe.
            let d = input.raw_scroll_delta;
            if d.x.abs() > d.y.abs() && d.x != 0.0 {
                let now = input.time;
                if now - self.swipe_at > 0.4 || self.swipe * d.x < 0.0 {
                    self.swipe = 0.0;
                }
                self.swipe += d.x;
                self.swipe_at = now;
                if self.swipe > 120.0 {
                    self.swipe = 0.0;
                    cmds.push(Command::Back);
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
            Command::PageNext => self.page(PAGE_STEP),
            Command::PagePrevious => self.page(-PAGE_STEP),
            Command::Last => self.to_end(),
            Command::Delete => self.delete_current(),
            Command::MarkUnread => self.mark_unread(),
            Command::Reply => self.reply(),
            Command::Send => self.send_reply(),
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
                self.conversation = None;
                self.body = None;
                self.web.hide();
            }
            // The headers again, off the message being answered. What has been
            // typed is the reader's and is not refreshed away.
            View::Compose { .. } => self.draft = None,
        }
        self.asked = None;
    }

    /// Writes a reply to the conversation on screen: the editor opens empty,
    /// with the address and the subject on their way.
    ///
    /// A reply answers the newest message on the conversation, whichever one
    /// the keyboard happens to be resting on — that is what "reply" means in a
    /// conversation, and it is what keeps it on one thread.
    fn reply(&mut self) {
        if self.nav.top().message().is_none() {
            return;
        }
        let (Some(account), Some(id)) = (
            self.account().map(str::to_owned),
            self.conversation
                .as_ref()
                .and_then(|thread| thread.newest())
                .map(|message| message.id.clone()),
        ) else {
            return;
        };
        self.compose.clear();
        self.to.clear();
        self.cc.clear();
        self.draft = None;
        self.sending = false;
        self.goto(View::Compose { account, id });
    }

    /// Sends what has been written. Two calls: the draft is rewritten with the
    /// typed body and the addresses as the fields have them, and that draft is
    /// sent — no dialog, because the key press that got here is the consent.
    fn send_reply(&mut self) {
        let View::Compose { account, id } = self.nav.top().clone() else {
            return;
        };
        if self.sending {
            return;
        }
        if self.compose.trim().is_empty() {
            self.set_status("Write something first.");
            return;
        }
        self.sending = true;
        self.start(Job::Draft {
            account,
            id,
            body: self.compose.clone(),
            to: Some(self.to.clone()),
            cc: Some(self.cc.clone()),
        });
    }

    /// Enter: the account the keyboard is on, the conversation it is on, or —
    /// inside one — the message it is on, opened or closed again.
    fn open_focused(&mut self) {
        if self.nav.top() == &View::Accounts {
            self.activate_account();
            return;
        }
        if self.nav.top().message().is_some() {
            self.toggle_message();
            return;
        }
        let (Some(account), Some(id)) = (
            self.account().map(str::to_owned),
            self.threads().get(self.focus).map(|thread| thread.id.clone()),
        ) else {
            return;
        };
        self.open(&account, &id);
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

    /// Opens one conversation. Showing it is what marks its unread messages
    /// read. `id` is the conversation's, or any message on it.
    pub fn open(&mut self, account: &str, id: &str) {
        self.goto(View::Detail {
            account: account.to_string(),
            id: id.to_string(),
        });
    }

    /// Down and Up: the next line in a list, the next message in a
    /// conversation. The same two keys, because in every view they mean
    /// "further on".
    fn step(&mut self, by: i32) {
        // In a reply the keys are the editor's, whether or not it has focus.
        if self.nav.top().replying_to().is_some() {
            return;
        }
        if self.nav.top().message().is_some() {
            // Arrows read the message: they scroll. The conversation is
            // walked by clicking its lines.
            self.body_scroll += by as f32 * BODY_STEP;
        } else if self.nav.top() == &View::Accounts {
            self.move_account_focus(by);
        } else {
            self.move_focus(by);
        }
    }

    /// PageDown and PageUp: a screenful of the list, and a screenful of the
    /// open message's body. Walking a conversation is the arrows' job; these
    /// are how the body under them moves without a mouse.
    fn page(&mut self, by: i32) {
        if self.nav.top().replying_to().is_some() {
            return;
        }
        if self.nav.top().message().is_some() {
            self.body_scroll += by as f32 * BODY_STEP;
        } else {
            self.step(by);
        }
    }

    /// Walks the conversation on screen, message by message, and asks the list
    /// to bring the one it landed on back into view.
    pub fn step_conversation(&mut self, by: i32) {
        let moved = self
            .conversation
            .as_mut()
            .is_some_and(|conversation| conversation.step(by));
        if moved {
            self.follow_focus = true;
        }
    }

    /// Enter on a message of the conversation: opens it, or closes it when it
    /// is the one already open. Only ever one is open, so the body pane always
    /// has exactly one message to draw.
    ///
    /// The body opens under that message's own line, so the line is brought
    /// into view: expanding one at the bottom of a long conversation must not
    /// open it off screen.
    pub fn toggle_message(&mut self) {
        if let Some(conversation) = &mut self.conversation {
            conversation.toggle();
        }
        self.follow_focus = true;
        self.show_open_message();
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

    /// Moves the keyboard's line, and asks the list to bring it back into view.
    ///
    /// Landing on the last conversation loaded — or pressing on past it — is
    /// the ask for the page after this one, which goes on the end.
    pub fn move_focus(&mut self, by: i32) {
        let rows = self.threads().len();
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

    /// End: the last line loaded, and the page after it. In a conversation the
    /// key is the body's, not the list's.
    fn to_end(&mut self) {
        if self.nav.top().message().is_some() || self.nav.top().replying_to().is_some() {
            return;
        }
        if self.nav.top() == &View::Accounts {
            self.move_account_focus(self.account_stops() as i32);
            return;
        }
        let rows = self.threads().len();
        if rows == 0 {
            return;
        }
        if self.focus != rows - 1 {
            self.focus = rows - 1;
            self.follow_focus = true;
        }
        self.load_more();
    }

    /// Trashes the message the keyboard is on in a conversation, or the whole
    /// conversation the keyboard is on in the list. Nothing at all while a
    /// reply is being written: there is no message under this view.
    fn delete_current(&mut self) {
        let Some(account) = self.account().map(str::to_owned) else {
            return;
        };
        if self.nav.top().replying_to().is_some() {
            return;
        }
        let ids: Vec<String> = match self.nav.top().message() {
            Some(_) => self
                .conversation
                .as_ref()
                .and_then(|thread| thread.focused())
                .map(|message| vec![message.id.clone()])
                .unwrap_or_default(),
            None => self
                .threads()
                .get(self.focus)
                .map(|thread| thread.ids.clone())
                .unwrap_or_default(),
        };
        for id in ids {
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

    /// Puts the unread flag back on the message the keyboard is on.
    fn mark_unread(&mut self) {
        if self.nav.top().message().is_none() {
            return;
        }
        let (Some(account), Some(id)) = (
            self.account().map(str::to_owned),
            self.conversation
                .as_ref()
                .and_then(|thread| thread.focused())
                .map(|message| message.id.clone()),
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
            View::Detail { account, id } => (!self.showing(id)).then(|| Job::Thread {
                account: account.clone(),
                id: id.clone(),
            }),
            // A reply with nothing in it yet: drafted only to read the address
            // and the subject off the message being answered. Nothing is sent,
            // and the body that replaces it is the one the reader types.
            View::Compose { account, id } => self.draft.is_none().then(|| Job::Draft {
                account: account.clone(),
                id: id.clone(),
                body: String::new(),
                to: None,
                cc: None,
            }),
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
            View::Detail { account, .. } => match &self.conversation {
                Some(thread) if !thread.subject.is_empty() => {
                    format!("Post — {}", thread.subject)
                }
                _ => format!("Post — {account}"),
            },
            View::Compose { account, .. } => match &self.draft {
                Some(draft) if !draft.subject.is_empty() => format!("Post — {}", draft.subject),
                _ => format!("Post — reply from {account}"),
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
                    "thread_id": format!("18f{i}"),
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

    // --------------------------------------------------------------- replying

    /// A reply, exactly as Post's own library addresses one.
    fn draft() -> crate::client::Draft {
        crate::client::Draft {
            id: "reply-18f7".to_string(),
            to: "ale@acme.com".to_string(),
            cc: "cm@ey.com".to_string(),
            subject: "Re: the quote".to_string(),
            attribution: "On 2025-09-12 10:33, Ale <ale@acme.com> wrote:".to_string(),
            quote: "Can you send the quote?".to_string(),
        }
    }

    /// A conversation of three messages, exactly as `post_thread` answers it:
    /// the message asked for, a reply of this account's own, and the newest.
    fn conversation(id: &str) -> crate::client::Conversation {
        crate::client::Conversation::of_answer(
            "ed@acme.com",
            &json!({
                "account": "ed@acme.com",
                "thread_id": id,
                "subject": "Re: the quote",
                "messages": [
                    {"id": format!("{id}-a"), "from": "Ale <ale@acme.com>",
                     "date": "2025-09-10 19:20", "unread": false, "text": "The quote?"},
                    {"id": format!("{id}-b"), "from": "Ed <ed@acme.com>",
                     "date": "2025-09-12 09:10", "unread": false, "text": "On its way."},
                    {"id": id, "from": "Ale <ale@acme.com>",
                     "date": "2025-09-12 10:33", "unread": true, "text": "Looks good."}
                ]
            }),
        )
    }

    /// A conversation open on screen.
    fn reading(id: &str) -> PostApp {
        let mut app = inbox(40);
        app.open("ed@acme.com", id);
        app.absorb(Done::Thread(conversation(id)));
        app
    }

    /// A conversation open, and Reply pressed on it.
    fn replying() -> PostApp {
        let mut app = reading("18f7");
        app.reply();
        app
    }

    /// Reply puts the editor over the message, empty, with the message behind
    /// it: the body pane is gone and Back goes to what is being answered.
    #[test]
    fn reply_opens_an_empty_editor_over_the_message() {
        let app = replying();
        assert_eq!(
            app.nav.top(),
            &View::Compose {
                account: "ed@acme.com".to_string(),
                id: "18f7".to_string(),
            }
        );
        assert_eq!(app.nav.depth(), 3, "the inbox, the message, the reply");
        assert!(app.compose.is_empty(), "a reply starts empty");
        assert!(app.draft.is_none(), "and unaddressed until the draft lands");
        assert!(!app.sending);
        // A reply is not the message: nothing here is the body pane's.
        assert!(app.nav.top().message().is_none());
        assert_eq!(app.nav.top().replying_to(), Some("18f7"));

        // Nothing to reply to is not a reply: from the list, Reply does
        // nothing rather than composing to whatever was selected.
        let mut list = inbox(40);
        list.reply();
        assert_eq!(
            list.nav.top(),
            &View::Inbox {
                account: "ed@acme.com".to_string()
            }
        );
    }

    /// The draft says who the reply goes to and what it is about. It does not
    /// touch what has been typed: the editor owns the body.
    #[test]
    fn the_draft_addresses_the_reply_without_touching_it() {
        let mut app = replying();
        app.compose = "Looks good — send it.".to_string();
        app.absorb(Done::Drafted(draft()));

        let addressed = app.draft.as_ref().expect("a draft");
        assert_eq!(addressed.to, "ale@acme.com");
        assert_eq!(addressed.subject, "Re: the quote");
        assert_eq!(app.compose, "Looks good — send it.", "untouched");
        assert!(!app.sending, "nothing was asked to be sent");
        assert!(!app.jobs.busy(), "and nothing was started");

        // The reply answers everybody, and the fields say so — they are the
        // reader's from here.
        assert_eq!(app.to, "ale@acme.com");
        assert_eq!(app.cc, "cm@ey.com");
    }

    /// The addresses are filled once. What the reader types over them is what
    /// is sent: the draft that comes back on the way out does not write over
    /// the fields it was built from.
    #[test]
    fn the_address_fields_are_the_readers_once_they_are_filled() {
        let mut app = replying();
        app.absorb(Done::Drafted(draft()));
        app.to = "fredrik@norberg.se".to_string();
        app.cc.clear();
        app.compose = "Ses där.".to_string();

        app.send_reply();
        assert!(app.sending);

        app.absorb(Done::Drafted(draft()));
        assert_eq!(app.to, "fredrik@norberg.se", "not written back over");
        assert!(app.cc.is_empty());
    }

    /// Sending is two steps: the draft is rewritten with what was typed, and
    /// that draft is what goes. An empty reply is not sent at all.
    #[test]
    fn sending_writes_the_typed_body_down_first() {
        let mut app = replying();
        app.absorb(Done::Drafted(draft()));

        app.set_status("");
        app.send_reply();
        assert!(!app.sending, "an empty reply is not a reply");
        assert!(!app.jobs.busy());
        assert!(app.status.contains("Write something"), "got {}", app.status);

        app.compose = "Looks good — send it.".to_string();
        app.send_reply();
        assert!(app.sending);
        assert_eq!(app.status, Job::Draft {
            account: "ed@acme.com".to_string(),
            id: "18f7".to_string(),
            body: "Looks good — send it.".to_string(),
            to: Some("ale@acme.com".to_string()),
            cc: Some("cm@ey.com".to_string()),
        }
        .about());

        // The draft that comes back while sending is the one to send.
        app.absorb(Done::Drafted(draft()));
        assert!(app.sending, "still on its way");
        assert_eq!(app.draft.as_ref().expect("a draft").id, "reply-18f7");
    }

    /// Sent: the editor empties and the reader is back at the message they
    /// answered, with one line saying where it went.
    #[test]
    fn a_sent_reply_goes_back_to_the_message() {
        let mut app = replying();
        app.compose = "Looks good.".to_string();
        app.sending = true;
        app.absorb(Done::Sent {
            to: "ale@acme.com".to_string(),
        });

        assert!(!app.sending);
        assert!(app.compose.is_empty(), "nothing is left in the editor");
        assert!(app.draft.is_none());
        assert_eq!(
            app.nav.top(),
            &View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f7".to_string(),
            }
        );
        assert!(app.status.contains("ale@acme.com"), "got {}", app.status);
    }

    /// A reply that did not go keeps every word of itself: the reader can fix
    /// whatever it was and send again.
    #[test]
    fn a_reply_that_failed_is_still_written() {
        let mut app = replying();
        app.compose = "Looks good.".to_string();
        app.sending = true;
        app.absorb(Done::Failed {
            about: "Sending…".to_string(),
            error: "Gmail refused the call (403)".to_string(),
        });

        assert!(!app.sending, "and the window is not left waiting");
        assert_eq!(app.compose, "Looks good.");
        assert_eq!(
            app.nav.top(),
            &View::Compose {
                account: "ed@acme.com".to_string(),
                id: "18f7".to_string(),
            },
            "still in the reply, where the words are"
        );
        assert!(app.status.contains("403"), "got {}", app.status);
    }

    /// The keys that act on a message do nothing to a reply: there is no row
    /// under this view to trash, and no body of someone else's to scroll.
    #[test]
    fn the_message_keys_do_nothing_while_a_reply_is_open() {
        let mut app = replying();
        app.focus = 7;

        app.step(1);
        assert_eq!(app.body_scroll, 0.0, "nothing to scroll here");
        assert_eq!(app.focus, 7, "and nothing to select");

        app.to_end();
        assert_eq!(app.focus, 7);

        app.set_status("");
        app.delete_current();
        assert!(!app.jobs.busy(), "nothing was trashed");
        assert_eq!(app.status, "");
    }

    /// In a conversation the same two keys walk its messages instead, and
    /// Enter opens the one they land on. The selection in the list behind
    /// stays exactly where the reader left it.
    #[test]
    fn in_a_conversation_the_arrows_walk_its_messages() {
        let mut app = reading("18f7");
        app.focus = 7;
        let thread = app.conversation.as_ref().expect("a conversation");
        assert_eq!(thread.focus, 2, "opened at the newest");
        assert_eq!(thread.open, Some(2));

        app.step(-1);
        let thread = app.conversation.as_ref().expect("a conversation");
        assert_eq!(thread.focus, 1, "up walks back through the conversation");
        assert_eq!(thread.open, Some(2), "and opens nothing on the way");
        assert_eq!(app.focus, 7, "the list behind it did not move");
        assert!(app.follow_focus, "the conversation follows the keyboard");

        app.exec(Command::Open, &eframe::egui::Context::default());
        let thread = app.conversation.as_ref().expect("a conversation");
        assert_eq!(thread.open, Some(1), "Enter opens the one it is on");
        assert_eq!(app.message().expect("a message").id, "18f7-b");

        app.exec(Command::Open, &eframe::egui::Context::default());
        assert!(app.message().is_none(), "and closes it again");

        // The page keys are what moves a body without a mouse.
        app.page(1);
        assert!(app.body_scroll > 0.0, "page down scrolls down");
        let down = app.body_scroll;
        app.page(-1);
        assert_eq!(app.body_scroll, 0.0, "page up undoes it");
        // The frame that scrolls takes it; the next one starts from nothing.
        app.body_scroll = down;
        let taken = std::mem::take(&mut app.body_scroll);
        assert!(taken != 0.0);
        assert_eq!(app.body_scroll, 0.0);

        // Back in the list, the arrows move the selection again.
        app.back();
        app.step(1);
        assert_eq!(app.focus, 8);
        assert_eq!(app.body_scroll, 0.0);
    }

    /// Reply answers the newest message on the conversation, whichever one the
    /// keyboard is resting on — that is what keeps it on one thread.
    #[test]
    fn reply_answers_the_newest_message_on_the_conversation() {
        let mut app = reading("18f7");
        app.step_conversation(-2);
        assert_eq!(
            app.conversation.as_ref().expect("a conversation").focus,
            0,
            "the keyboard is on the oldest"
        );
        app.reply();
        assert_eq!(
            app.nav.top(),
            &View::Compose {
                account: "ed@acme.com".to_string(),
                id: "18f7".to_string(),
            },
            "and the reply still answers the newest"
        );
    }
}
