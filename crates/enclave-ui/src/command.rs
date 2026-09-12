//! What the keymap and the ribbon need from an app's command list.
//!
//! Each app keeps its own command enum: the ids, the labels and the dispatch
//! are its business. This trait is the part the shared machinery uses — an id
//! resolves to a command, and a command names itself for menus and help.

/// A user-invocable action with a stable string id.
pub trait CommandId: Copy + PartialEq {
    /// The command with this id, or None when nothing answers to it.
    fn from_id(id: &str) -> Option<Self>
    where
        Self: Sized;

    /// Human-readable name, used by menus and the shortcut help window.
    fn label(self) -> &'static str;
}
