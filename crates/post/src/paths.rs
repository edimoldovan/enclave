//! Where Post keeps things.
//!
//! Three levels, and only the middle one is a person's. Application config —
//! the Google OAuth client — is embedded at build time and never on this
//! computer. User config is small and hand-editable in `~/.config/enclave/`,
//! shared by the whole suite rather than scoped to a product: the connected
//! accounts and one OAuth token file each. Anything rebuildable — fetched HTML
//! bodies — lives in the state dir instead, where deleting it costs nothing.
//! Attachments land in `~/Enclaved/`, next to the files colleagues drop.

use std::path::{Path, PathBuf};

/// The user's home, or /tmp when there is no telling — the same fallback the
/// allowlist uses, so nothing here ever writes to the root of the filesystem.
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// `$XDG_CONFIG_HOME/enclave`, or `~/.config/enclave`: the user config dir.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
        .join("enclave")
}

/// `$XDG_DATA_HOME/enclave`, or `~/.local/share/enclave`: the state dir.
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("enclave")
}

/// Where an earlier version kept its config, one directory deeper. Config is
/// the person's, not the product's, so what is left down there moves up.
fn older_dir(config: &Path) -> PathBuf {
    config.join("post")
}

/// The connected Google accounts: `[{"email": "…"}]`.
pub fn accounts_file() -> PathBuf {
    accounts_file_in(&config_dir())
}

/// One OAuth token file per account, 0600 in a 0700 directory.
pub fn oauth_tokens_dir() -> PathBuf {
    oauth_tokens_dir_in(&config_dir())
}

/// The account list under `config`.
fn accounts_file_in(config: &Path) -> PathBuf {
    migrate_user_config(config);
    config.join("accounts.json")
}

/// The OAuth token directory under `config`.
fn oauth_tokens_dir_in(config: &Path) -> PathBuf {
    migrate_user_config(config);
    config.join("oauth-tokens")
}

/// Moves what an earlier version left one directory deeper up into `config`,
/// once each. Both moves run on the first path asked for, so a computer is
/// never left half-moved. The OAuth token directory had an earlier name still
/// — it was called `tokens` before it said what kind — and either one moves.
fn migrate_user_config(config: &Path) {
    let older = older_dir(config);
    if !older.is_dir() {
        return;
    }
    crate::store::migrate(&older.join("accounts.json"), &config.join("accounts.json"));
    let oauth_tokens = config.join("oauth-tokens");
    crate::store::migrate(&older.join("oauth-tokens"), &oauth_tokens);
    crate::store::migrate(&older.join("tokens"), &oauth_tokens);
    // Empty now, so it goes. Anything else left down there keeps it.
    let _ = std::fs::remove_dir(&older);
}

/// Fetched HTML bodies, one directory per account. Rebuildable: the provider
/// stays authoritative.
pub fn bodies_dir() -> PathBuf {
    state_dir().join("postbodies")
}

/// The newest fetched page of each mailbox, one file per account. Rebuildable
/// like the bodies: it is a head start, never the truth.
pub fn list_cache_dir() -> PathBuf {
    state_dir().join("postlist")
}

/// One account's cached page. An address is a filename here, so nothing in it
/// may be a path.
pub fn list_cache_file(email: &str) -> PathBuf {
    list_cache_dir().join(format!("{}.json", safe(email)))
}

/// Messages already fetched, one directory per account. A message never
/// changes, so a message read once is read from here forever after.
pub fn read_cache_dir() -> PathBuf {
    state_dir().join("postread")
}

/// One message's cached read. Both the address and the id are filenames here,
/// so nothing in either may be a path.
pub fn read_cache_file(email: &str, id: &str) -> PathBuf {
    read_cache_dir()
        .join(safe(email))
        .join(format!("{}.json", safe(id)))
}

/// A name as a filename: nothing in it is allowed to be a path.
fn safe(name: &str) -> String {
    name.chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect()
}

/// Where a saved attachment lands.
pub fn attachments_dir() -> PathBuf {
    home().join("Enclaved").join("Attachments")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, OauthTokens};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("post-paths-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        dir
    }

    fn some_oauth_tokens() -> OauthTokens {
        OauthTokens {
            refresh_token: "1//refresh".to_string(),
            access_token: "ya29.access".to_string(),
            expiry: 1_757_000_000,
        }
    }

    /// A fresh computer: the paths are right under the config dir, and nothing
    /// is conjured on the way.
    #[test]
    fn with_nothing_to_move_the_paths_are_the_config_dir_itself() {
        let config = scratch("fresh");
        assert_eq!(accounts_file_in(&config), config.join("accounts.json"));
        assert_eq!(oauth_tokens_dir_in(&config), config.join("oauth-tokens"));
        assert!(!config.join("accounts.json").exists());
        assert!(!config.join("oauth-tokens").exists());
    }

    /// A computer that connected an account before config stopped being
    /// product-scoped keeps it: the list moves up once.
    #[test]
    fn an_older_account_list_is_moved_up_once() {
        let config = scratch("accounts-move");
        let older = older_dir(&config).join("accounts.json");
        store::add_account(&older, "ed@acme.com").expect("added");

        let file = accounts_file_in(&config);
        assert_eq!(file, config.join("accounts.json"));
        assert!(!older.exists(), "the older list is gone");
        assert_eq!(store::accounts(&file), vec!["ed@acme.com"]);

        // Once: a stray left by an older copy still running does not win.
        store::add_account(&older, "stray@acme.com").expect("added");
        let file = accounts_file_in(&config);
        assert!(older.exists(), "the stray stays where it is");
        assert_eq!(store::accounts(&file), vec!["ed@acme.com"]);
    }

    /// The same for the OAuth tokens — and the moved file is still nobody
    /// else's to read. Asking for the account list is enough to move them:
    /// whichever path is asked for first, the computer is never left half
    /// moved.
    #[test]
    fn an_older_oauth_token_directory_is_moved_up_once() {
        let config = scratch("oauth-tokens-move");
        let older = older_dir(&config).join("oauth-tokens");
        let tokens = some_oauth_tokens();
        store::save_oauth_tokens(&older, "ed@acme.com", &tokens).expect("saved");
        store::add_account(&older_dir(&config).join("accounts.json"), "ed@acme.com")
            .expect("added");

        // Only the account list is asked for.
        let file = accounts_file_in(&config);
        assert_eq!(store::accounts(&file), vec!["ed@acme.com"]);
        assert!(!older.exists(), "the older directory is gone");
        assert!(
            !older_dir(&config).exists(),
            "and so is the emptied directory it sat in"
        );

        let dir = oauth_tokens_dir_in(&config);
        assert_eq!(dir, config.join("oauth-tokens"));
        assert_eq!(
            store::load_oauth_tokens(&dir, "ed@acme.com").expect("loaded"),
            tokens
        );
        assert_eq!(
            store::mode(&store::oauth_token_file(&dir, "ed@acme.com")),
            Some(0o600)
        );
        assert_eq!(store::mode(&dir), Some(0o700));
    }

    /// The oldest shape of all: a directory named before it said what kind of
    /// token it held. It moves up too.
    #[test]
    fn a_directory_from_before_the_oauth_rename_is_moved_up_too() {
        let config = scratch("oauth-tokens-oldest");
        let oldest = older_dir(&config).join("tokens");
        let tokens = some_oauth_tokens();
        store::save_oauth_tokens(&oldest, "ed@acme.com", &tokens).expect("saved");

        let dir = oauth_tokens_dir_in(&config);
        assert!(!oldest.exists(), "the oldest directory is gone");
        assert_eq!(
            store::load_oauth_tokens(&dir, "ed@acme.com").expect("loaded"),
            tokens
        );
        assert_eq!(store::mode(&dir), Some(0o700));
    }
}
