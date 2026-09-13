//! One executable, three roles, decided by the command line alone.

use std::path::PathBuf;

use enclave::role::{self, Role};

fn role(args: &[&str]) -> Role {
    role::of(args)
}

#[test]
fn the_assistant_launches_the_shim() {
    assert_eq!(role(&["mcp"]), Role::Shim);
}

#[test]
fn the_server_is_asked_for_by_flag() {
    assert_eq!(role(&["--serve"]), Role::Serve);
}

/// How the server puts a question on screen: this same binary, one dialog.
#[test]
fn the_dialog_is_asked_for_by_flag() {
    assert_eq!(role(&["--confirm"]), Role::Confirm);
}

#[test]
fn registering_stays_its_own_thing() {
    assert_eq!(role(&["register"]), Role::Register);
}

/// The terminal door onto a product's palette: everything after the product's
/// name is the verb and its arguments, flags included — and no window opens.
#[test]
fn a_product_name_first_means_its_verbs() {
    for (line, words) in [
        ("post accounts", vec!["accounts"]),
        ("post add-account", vec!["add-account"]),
        ("post mark ed@acme.com 18f3 --read", vec![
            "mark",
            "ed@acme.com",
            "18f3",
            "--read",
        ]),
        ("post delete ed@acme.com 18f3", vec!["delete", "ed@acme.com", "18f3"]),
        ("post attachment ed@acme.com 18f3", vec![
            "attachment",
            "ed@acme.com",
            "18f3",
        ]),
        // A page of a listing has no view to be, so it stays a printed page.
        ("post list ed@acme.com --page tok3n", vec![
            "list",
            "ed@acme.com",
            "--page",
            "tok3n",
        ]),
        // Misuse still reaches the palette, which is where the usage line is.
        ("post list", vec!["list"]),
        ("post read ed@acme.com", vec!["read", "ed@acme.com"]),
        ("post --help", vec!["--help"]),
        ("post sendmail", vec!["sendmail"]),
    ] {
        let args: Vec<&str> = line.split(' ').collect();
        assert_eq!(
            role(&args),
            Role::Post {
                words: words.into_iter().map(str::to_string).collect()
            },
            "{line}"
        );
    }
}

/// The three mail verbs that are views rather than answers open the window at
/// that view. Nobody reads email in a terminal.
#[test]
fn the_mail_verbs_that_are_views_open_the_window() {
    use post_ui::View;

    assert_eq!(
        role(&["post"]),
        Role::PostOpen {
            view: View::Accounts
        }
    );
    assert_eq!(
        role(&["post", "list", "ed@acme.com"]),
        Role::PostOpen {
            view: View::Inbox {
                account: "ed@acme.com".to_string()
            }
        }
    );
    // Not a file to open, and not the server either.
    assert_eq!(
        role(&["post", "read", "ed@acme.com", "18f3a2c9b1"]),
        Role::PostOpen {
            view: View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3a2c9b1".to_string(),
            }
        }
    );
}

/// And the line a launcher writes: the same three views, being the window
/// rather than asking for one. Nobody types this.
#[test]
fn the_window_itself_is_a_role_of_its_own() {
    use post_ui::View;

    assert_eq!(
        role(&["post", "--window"]),
        Role::PostWindow {
            view: View::Accounts
        }
    );
    assert_eq!(
        role(&["post", "--window", "list", "ed@acme.com"]),
        Role::PostWindow {
            view: View::Inbox {
                account: "ed@acme.com".to_string()
            }
        }
    );
    assert_eq!(
        role(&["post", "--window", "read", "ed@acme.com", "18f3a2c9b1"]),
        Role::PostWindow {
            view: View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3a2c9b1".to_string(),
            }
        }
    );
    // A flag that names no view is a verb the palette answers, not a window.
    assert_eq!(
        role(&["post", "--window", "sendmail"]),
        Role::Post {
            words: vec!["--window".to_string(), "sendmail".to_string()]
        }
    );
}

/// The common case: a person double-clicks a file, or types its name.
#[test]
fn a_file_means_a_window_for_the_default_product() {
    assert_eq!(
        role(&["sales.xlsx"]),
        Role::Window {
            product: "grido".to_string(),
            path: Some(PathBuf::from("sales.xlsx")),
        }
    );
    assert_eq!(
        role(&[]),
        Role::Window {
            product: "grido".to_string(),
            path: None,
        }
    );
}

/// How the server starts a window when a tool call arrives and none is open.
#[test]
fn a_product_can_be_named() {
    assert_eq!(
        role(&["--product", "grido"]),
        Role::Window {
            product: "grido".to_string(),
            path: None,
        }
    );
    assert_eq!(
        role(&["--product=grido", "/home/ed/q3.xlsx"]),
        Role::Window {
            product: "grido".to_string(),
            path: Some(PathBuf::from("/home/ed/q3.xlsx")),
        }
    );
    // The product's name is not mistaken for a file to open.
    assert_eq!(
        role(&["--product", "grido", "book.xlsx"]),
        Role::Window {
            product: "grido".to_string(),
            path: Some(PathBuf::from("book.xlsx")),
        }
    );
}

#[test]
fn flags_are_not_files_and_the_first_file_wins() {
    assert_eq!(
        role(&["--maximized", "one.xlsx", "two.xlsx"]),
        Role::Window {
            product: "grido".to_string(),
            path: Some(PathBuf::from("one.xlsx")),
        }
    );
}
