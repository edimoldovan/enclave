//! The three views, and the stack behind them.
//!
//! Post's window is one window with a back button, not a three-pane client:
//! accounts, then one account's inbox, then one message. Back retraces steps
//! that were actually taken: the window opens at one view and that view is the
//! floor, so `enclave post list ed@…` lands in an inbox with nothing behind
//! it, while tapping a row from that inbox puts the inbox behind the message.
//!
//! This module is the state machine, and it knows nothing about egui: it is
//! also where a command line turns into a view, so `enclave post list ed@…`
//! and a second launch handing that view over take the same road.

/// What the window is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// The connected accounts — where the window opens with no view asked for.
    Accounts,
    /// One account's mail.
    Inbox { account: String },
    /// One message of one account's mail.
    Detail { account: String, id: String },
}

impl View {
    /// The account this view is about, if any.
    pub fn account(&self) -> Option<&str> {
        match self {
            View::Accounts => None,
            View::Inbox { account } | View::Detail { account, .. } => Some(account),
        }
    }

    /// The message this view is about, if any.
    pub fn message(&self) -> Option<&str> {
        match self {
            View::Detail { id, .. } => Some(id),
            _ => None,
        }
    }

    /// The words that ask for this view — the inverse of [`View::of_words`],
    /// and what a process that is not the window passes to one it starts.
    ///
    /// Round-trips for every view whose account and id are words at all: an
    /// empty one, or one starting with `-`, is not a word the command line can
    /// carry, so the caller checks before sending a view down this road.
    pub fn words(&self) -> Vec<String> {
        match self {
            View::Accounts => Vec::new(),
            View::Inbox { account } => vec!["list".to_string(), account.clone()],
            View::Detail { account, id } => {
                vec!["read".to_string(), account.clone(), id.clone()]
            }
        }
    }

    /// The view `enclave post <words>` asks for — or `None` when those words
    /// are a terminal verb, or are not a verb at all.
    ///
    /// Only the three views have a command: everything else (`add-account`,
    /// `mark`, `delete`, `attachment`, `accounts`, the help, a typo) stays in
    /// the terminal, where it already says the right thing. A page token has no
    /// view to be, so `list --page …` stays a terminal listing too.
    pub fn of_words<S: AsRef<str>>(words: &[S]) -> Option<View> {
        let words: Vec<&str> = words.iter().map(AsRef::as_ref).collect();
        match words.as_slice() {
            [] => Some(View::Accounts),
            ["list", email] => Some(View::Inbox {
                account: value(email)?,
            }),
            ["read", email, id] => Some(View::Detail {
                account: value(email)?,
                id: value(id)?,
            }),
            _ => None,
        }
    }
}

/// A command-line word standing in for an address or an id. A flag is not one,
/// and neither is nothing at all: both belong to the terminal door, which says
/// what was missing.
fn value(word: &str) -> Option<String> {
    let word = word.trim();
    if word.is_empty() || word.starts_with('-') {
        return None;
    }
    Some(word.to_string())
}

/// The views behind the one on screen — the ones actually walked through.
/// Never empty: the view the window opened at is the floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nav {
    stack: Vec<View>,
}

impl Nav {
    /// A window opening at `view`, with nothing behind it: it was not walked
    /// to, so Back has nowhere to go until the reader walks somewhere.
    pub fn of(view: View) -> Nav {
        Nav { stack: vec![view] }
    }

    /// What is on screen.
    pub fn top(&self) -> &View {
        self.stack.last().unwrap_or(&View::Accounts)
    }

    /// How deep the stack is. One is the view the window opened at, where
    /// Back does nothing.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// A step the reader took: the view we were on stays behind this one.
    /// Idempotent, and stepping back onto a view already behind us returns to
    /// it rather than stacking it again, so walking in circles stays shallow.
    pub fn goto(&mut self, view: View) {
        if self.top() == &view {
            return;
        }
        if let Some(at) = self.stack.iter().position(|behind| behind == &view) {
            self.stack.truncate(at + 1);
            return;
        }
        self.stack.push(view);
    }

    /// Back one view. False at the floor, where there is nowhere to go.
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }
}

impl Default for Nav {
    fn default() -> Nav {
        Nav::of(View::Accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    /// The three commands that open the window, and only those three.
    #[test]
    fn the_command_line_names_the_view() {
        assert_eq!(View::of_words::<String>(&[]), Some(View::Accounts));
        assert_eq!(
            View::of_words(&words("list ed@acme.com")),
            Some(View::Inbox {
                account: "ed@acme.com".to_string()
            })
        );
        assert_eq!(
            View::of_words(&words("read ed@acme.com 18f3a2c9b1")),
            Some(View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3a2c9b1".to_string(),
            })
        );
    }

    /// And back out: a view names the words that ask for it, so a launch that
    /// starts a window lands exactly where the caller meant.
    #[test]
    fn a_view_names_the_command_line_that_asks_for_it() {
        for view in [
            View::Accounts,
            View::Inbox {
                account: "ed@acme.com".to_string(),
            },
            View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3a2c9b1".to_string(),
            },
        ] {
            assert_eq!(View::of_words(&view.words()), Some(view.clone()), "{view:?}");
        }
        assert_eq!(
            View::Inbox {
                account: "ed@acme.com".to_string()
            }
            .words(),
            vec!["list".to_string(), "ed@acme.com".to_string()]
        );
        // What cannot be a word cannot be a command line: the round trip says
        // so rather than quietly asking for a different view.
        assert_eq!(
            View::of_words(
                &View::Inbox {
                    account: "--nope".to_string()
                }
                .words()
            ),
            None
        );
    }

    /// Everything else stays in the terminal — the scripting verbs, the browser
    /// flow, the help, and anything misspelled. The window never swallows a
    /// command line the terminal already answers well.
    #[test]
    fn the_terminal_verbs_open_nothing() {
        for line in [
            "accounts",
            "add-account",
            "mark ed@acme.com 18f3 --read",
            "delete ed@acme.com 18f3",
            "attachment ed@acme.com 18f3 quote.pdf",
            "help",
            "--help",
            "-h",
            "sendmail",
            // A page of a listing is not a view.
            "list ed@acme.com --page tok3n",
        ] {
            assert_eq!(View::of_words(&words(line)), None, "\"{line}\" opened a window");
        }
    }

    /// Misuse must still reach the terminal, which is where the usage line
    /// lives: a missing address or id is not a window with nothing in it.
    #[test]
    fn a_command_line_missing_its_words_opens_nothing() {
        for line in ["list", "read", "read ed@acme.com", "list --page", "read -- 18f3"] {
            assert_eq!(View::of_words(&words(line)), None, "\"{line}\" opened a window");
        }
        // A flag where an address belongs is a flag, not an address.
        assert_eq!(View::of_words(&words("list --nope")), None);
    }

    /// The view a window opens at is the floor, whichever view it is: nothing
    /// was walked through to get there, so Back does nothing.
    #[test]
    fn the_view_a_window_opens_at_has_nothing_behind_it() {
        for view in [
            View::Accounts,
            View::Inbox {
                account: "ed@acme.com".to_string(),
            },
            View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string(),
            },
        ] {
            let mut nav = Nav::of(view.clone());
            assert_eq!(nav.depth(), 1, "{view:?}");
            assert_eq!(nav.top(), &view);
            assert!(!nav.back(), "{view:?} was opened at, not walked to");
            assert_eq!(nav.top(), &view);
        }
    }

    /// Walking is what Back retraces: a message opened from an inbox has that
    /// inbox behind it, and only the steps taken are on the stack.
    #[test]
    fn back_retraces_the_steps_that_were_taken() {
        let mut nav = Nav::of(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        nav.goto(View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        });
        assert_eq!(nav.depth(), 2);
        assert!(nav.back());
        assert_eq!(
            nav.top(),
            &View::Inbox {
                account: "ed@acme.com".to_string()
            }
        );
        assert!(!nav.back(), "the inbox it opened at is the floor");

        // From the accounts, the whole road is walked and Back unwinds it.
        let mut nav = Nav::of(View::Accounts);
        nav.goto(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        nav.goto(View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        });
        assert_eq!(nav.depth(), 3);
        assert!(nav.back());
        assert!(nav.back());
        assert_eq!(nav.top(), &View::Accounts);
        assert!(!nav.back(), "the accounts are the floor here");
    }

    /// Walking in circles does not pile up: a view already behind us is
    /// returned to, and arriving where we already are is not a move at all.
    #[test]
    fn going_back_to_a_view_behind_us_returns_to_it() {
        let mut nav = Nav::of(View::Accounts);
        nav.goto(View::Inbox {
            account: "ed@acme.com".to_string(),
        });
        nav.goto(View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        });
        nav.goto(View::Accounts);
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.top(), &View::Accounts);

        // Another account is a step of its own.
        nav.goto(View::Inbox {
            account: "ale@acme.com".to_string(),
        });
        assert_eq!(nav.depth(), 2);
        assert_eq!(nav.top().account(), Some("ale@acme.com"));

        let before = nav.clone();
        nav.goto(View::Inbox {
            account: "ale@acme.com".to_string(),
        });
        assert_eq!(nav, before);
    }

    #[test]
    fn a_view_says_what_it_is_about() {
        let detail = View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3".to_string(),
        };
        assert_eq!(detail.account(), Some("ed@acme.com"));
        assert_eq!(detail.message(), Some("18f3"));
        assert_eq!(View::Accounts.account(), None);
        assert_eq!(
            View::Inbox {
                account: "ed@acme.com".to_string()
            }
            .message(),
            None
        );
    }
}
