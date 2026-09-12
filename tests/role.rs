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

#[test]
fn registering_stays_its_own_thing() {
    assert_eq!(role(&["register"]), Role::Register);
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
