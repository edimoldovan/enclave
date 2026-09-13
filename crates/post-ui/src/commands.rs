//! Every user-invocable action, with a stable string id.
//!
//! `keymap.toml` binds chords to these ids and the ribbon dispatches the same
//! commands, so a button and a key press are the same thing happening.

use enclave_ui::command::CommandId;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    /// Back one view: message → inbox → accounts.
    Back,
    /// The accounts, from wherever we are.
    Accounts,
    /// Ask the provider again for whatever is on screen.
    Refresh,
    /// Open the focused row.
    Open,
    /// Move the focus down a row.
    Next,
    /// Move the focus up a row.
    Previous,
    /// Move the focus down a screenful, fetching more mail at the end.
    PageNext,
    /// Move the focus up a screenful.
    PagePrevious,
    /// The last row loaded, and the page after it.
    Last,
    /// Move the message on screen — or the focused row — to the trash.
    Delete,
    /// Put the unread flag back on the message on screen.
    MarkUnread,
    /// Connect a Gmail account through Google's own consent screen.
    AddAccount,
    /// The shortcut viewer.
    ShortcutHelp,
    /// Close the window.
    Quit,
}

impl Command {
    pub fn from_id(id: &str) -> Option<Command> {
        use Command::*;
        Some(match id {
            "back" => Back,
            "accounts" => Accounts,
            "refresh" => Refresh,
            "open" => Open,
            "next" => Next,
            "previous" => Previous,
            "page_next" => PageNext,
            "page_previous" => PagePrevious,
            "last" => Last,
            "delete" => Delete,
            "mark_unread" => MarkUnread,
            "add_account" => AddAccount,
            "shortcut_help" => ShortcutHelp,
            "quit" => Quit,
            _ => return None,
        })
    }

    /// What the menus, the tooltips and the shortcut viewer call it.
    pub fn label(self) -> &'static str {
        use Command::*;
        match self {
            Back => "Back",
            Accounts => "Accounts",
            Refresh => "Refresh",
            Open => "Open",
            Next => "Next message",
            Previous => "Previous message",
            PageNext => "Down a page",
            PagePrevious => "Up a page",
            Last => "End of the list",
            Delete => "Delete",
            MarkUnread => "Mark unread",
            AddAccount => "Add account",
            ShortcutHelp => "Keyboard shortcuts",
            Quit => "Close window",
        }
    }

    /// Every command there is, in the order the shortcut viewer lists them.
    pub const ALL: [Command; 14] = [
        Command::Back,
        Command::Accounts,
        Command::Refresh,
        Command::Open,
        Command::Next,
        Command::Previous,
        Command::PageNext,
        Command::PagePrevious,
        Command::Last,
        Command::Delete,
        Command::MarkUnread,
        Command::AddAccount,
        Command::ShortcutHelp,
        Command::Quit,
    ];

    /// The id this command answers to in `keymap.toml`.
    pub fn id(self) -> &'static str {
        use Command::*;
        match self {
            Back => "back",
            Accounts => "accounts",
            Refresh => "refresh",
            Open => "open",
            Next => "next",
            Previous => "previous",
            PageNext => "page_next",
            PagePrevious => "page_previous",
            Last => "last",
            Delete => "delete",
            MarkUnread => "mark_unread",
            AddAccount => "add_account",
            ShortcutHelp => "shortcut_help",
            Quit => "quit",
        }
    }
}

impl CommandId for Command {
    fn from_id(id: &str) -> Option<Command> {
        Command::from_id(id)
    }

    fn label(self) -> &'static str {
        Command::label(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::KeyboardShortcut;

    /// Every command is reachable by its id, and every id round-trips — a
    /// keymap can name all of them and nothing is bound to a ghost.
    #[test]
    fn every_command_answers_to_its_id() {
        for cmd in Command::ALL {
            assert_eq!(Command::from_id(cmd.id()), Some(cmd), "{}", cmd.id());
            assert!(!cmd.label().is_empty());
        }
        assert_eq!(Command::from_id("sendmail"), None);
    }

    /// Chord and command id, as the shipped keymap spells them.
    fn shipped() -> Vec<(&'static str, &'static str)> {
        crate::keymap::DEFAULT_KEYMAP
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(chord, id)| (chord.trim().trim_matches('"'), id.trim().trim_matches('"')))
            .collect()
    }

    /// The shipped keymap binds only commands that exist, and binds the five
    /// the window is steered with.
    #[test]
    fn the_default_keymap_names_real_commands() {
        let bound: Vec<&str> = shipped().into_iter().map(|(_, id)| id).collect();
        assert!(!bound.is_empty(), "the keymap binds something");
        for id in &bound {
            assert!(Command::from_id(id).is_some(), "no command called \"{id}\"");
        }
        for needed in ["back", "refresh", "delete", "open", "accounts", "shortcut_help"] {
            assert!(bound.contains(&needed), "nothing is bound to {needed}");
        }
    }

    /// The four arrows steer the window on their own: right and left go in and
    /// back out, down and up move along. Left is Back with no modifier —
    /// reaching for Alt to leave a message is a keyboard nobody has.
    #[test]
    fn the_arrows_steer_the_window() {
        let bound = shipped();
        for (chord, id) in [
            ("ArrowLeft", "back"),
            ("ArrowRight", "open"),
            ("ArrowDown", "next"),
            ("ArrowUp", "previous"),
        ] {
            assert!(
                bound.contains(&(chord, id)),
                "{chord} is not bound to {id}"
            );
            assert!(
                enclave_ui::keymap::parse_chord(chord).is_some(),
                "{chord} is not a chord egui can match"
            );
        }
        // Escape and Backspace still go back too: Left joins them, it does not
        // replace them.
        assert!(bound.contains(&("Escape", "back")));
        assert!(bound.contains(&("Backspace", "back")));
    }

    /// Whether a chord's key is a function key — F1, Ctrl+F5 — as opposed to a
    /// letter that merely starts with an F.
    fn function_key(chord: &str) -> bool {
        chord.rsplit('+').next().is_some_and(|key| {
            key.len() >= 2
                && key.starts_with('F')
                && key[1..].chars().all(|c| c.is_ascii_digit())
        })
    }

    /// No command is reachable only by a function key. F1 is the old hand's
    /// alias for the shortcut viewer, and an alias is a second way to a
    /// command, never the only one — a keyboard without a function row still
    /// drives the whole window.
    #[test]
    fn no_command_is_reachable_only_by_a_function_key() {
        assert!(function_key("F1") && function_key("Ctrl+F5"));
        assert!(!function_key("Escape") && !function_key("Ctrl+F") && !function_key("F"));

        let bound = shipped();
        for (chord, id) in &bound {
            if !function_key(chord) {
                continue;
            }
            assert!(
                bound
                    .iter()
                    .any(|(other, also)| also == id && !function_key(other)),
                "\"{id}\" is only reachable by {chord}"
            );
        }
    }

    /// One chord means one thing. Two commands on the same key press would both
    /// run, in whatever order the file happened to list them — and the chords
    /// are compared as egui matches them, so two spellings of the same press
    /// are the same press.
    #[test]
    fn no_chord_means_two_things() {
        let mut seen: Vec<(KeyboardShortcut, &str, &str)> = Vec::new();
        for (chord, id) in shipped() {
            let pressed = enclave_ui::keymap::parse_chord(chord)
                .unwrap_or_else(|| panic!("\"{chord}\" is not a chord egui can match"));
            if let Some((_, spelt, already)) = seen.iter().find(|(key, ..)| *key == pressed) {
                assert_eq!(
                    *already, id,
                    "{chord} and {spelt} are the same key press, bound to two commands"
                );
            }
            seen.push((pressed, chord, id));
        }
        assert!(seen.len() > 10, "the keymap binds the window, not a token key");
    }
}
