//! Enclave Post's window — the third door onto the mail palette.
//!
//! There are already two: the assistant's `post_` tools and `enclave post
//! <verb>` in a terminal. This is the one for reading, because nobody reads
//! email in a terminal. Four commands open it and each one *is* a view —
//! `enclave post` the accounts, `enclave post list <email>` that account's
//! inbox, `enclave post read <email> <id>` that message, `enclave post reply
//! <email> <id>` a reply to it — so the window needs no launcher and no menu
//! to be useful: the command is the navigation.
//!
//! It is only a view. The mail lives in [`post`], which is headless and stays
//! that way: nothing here is required for mail to work, closing the window
//! loses nothing, and every verb the window runs is a verb the assistant and
//! the terminal run too.
//!
//! - [`nav`] — the four views and the stack, including which command line
//!   means which view. Pure, and where the behaviour is pinned down.
//! - [`client`] — the only file that calls the mail library; hands up plain
//!   types and runs the network on threads of its own.
//! - [`state`] — `PostApp` and every way an answer changes what is on screen.
//! - [`app`] — the update loop and command dispatch.
//! - [`commands`] / [`keymap`] — the registry and the chords bound to it.
//! - [`ui`] — one `impl PostApp` block per area of the interface, including
//!   [`ui::compose`], the reply editor.
//! - [`web`] — the HTML body pane, and the display-server truth behind it.

pub mod app;
pub mod client;
pub mod commands;
pub mod keymap;
pub mod nav;
pub mod state;
pub mod ui;
pub mod web;

pub use nav::View;
pub use state::PostApp;
