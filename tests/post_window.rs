//! One Post window, and where a second `enclave post …` goes.
//!
//! All of this runs with no display: the handover is a socket and a channel,
//! and the view that travels on it is decided before anything is drawn. What is
//! checked here is the part a person notices — typing the command twice gives
//! one window that navigates, and the second command exits instead of waiting.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use enclave::ipc::{self, Msg};
use enclave::posthost::{self, Outcome};
use enclave::role::{self, Role};
use post_ui::View;

/// A scratch runtime directory, standing in for `$XDG_RUNTIME_DIR`.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("enclave-tests")
        .join(format!("post-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    dir
}

/// The view the window was handed, or nothing within a second.
fn handed(rx: &std::sync::mpsc::Receiver<View>) -> Option<View> {
    rx.recv_timeout(Duration::from_secs(1)).ok()
}

fn inbox(account: &str) -> View {
    View::Inbox {
        account: account.to_string(),
    }
}

/// The first launch is the window; the second gives it the view and goes.
#[test]
fn the_second_command_navigates_the_window_that_is_already_up() {
    let dir = scratch("handoff");
    let socket = dir.join("enclave-post.sock");

    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };
    assert!(socket.exists(), "the window took the socket");

    // A second `enclave post list ed@acme.com`.
    match posthost::start_at(&socket, &inbox("ed@acme.com")) {
        Outcome::HandedOff => {}
        Outcome::Run(_) => panic!("a second window opened"),
    }
    assert_eq!(handed(&rx), Some(inbox("ed@acme.com")));

    // And a third, at a message this time.
    let detail = View::Detail {
        account: "ed@acme.com".to_string(),
        id: "18f3a2c9b1".to_string(),
    };
    assert!(matches!(
        posthost::start_at(&socket, &detail),
        Outcome::HandedOff
    ));
    assert_eq!(handed(&rx), Some(detail));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A socket file left behind by a crash answers nothing, so it is replaced —
/// never respected, and never left to make a launch hang.
#[test]
fn a_socket_a_crash_left_behind_is_taken_over() {
    let dir = scratch("stale");
    let socket = dir.join("enclave-post.sock");
    std::fs::write(&socket, b"not a socket").expect("a stale file");

    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the stale file should not hold the door");
    };
    assert!(matches!(
        posthost::start_at(&socket, &inbox("ed@acme.com")),
        Outcome::HandedOff
    ));
    assert_eq!(handed(&rx), Some(inbox("ed@acme.com")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A view arrives on the socket as two fields, and comes back out the same
/// view — that is the whole wire.
#[test]
fn a_view_travels_as_an_op_and_arrives_as_itself() {
    let dir = scratch("framing");
    let socket = dir.join("enclave-post.sock");
    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };

    let mut stream = UnixStream::connect(&socket).expect("the window answers");
    ipc::send(
        &mut stream,
        &Msg::View {
            id: Some(9),
            account: Some("ale@acme.com".to_string()),
            message: Some("18f2".to_string()),
        },
    )
    .expect("sent");
    let mut reader = BufReader::new(stream.try_clone().expect("a reader"));
    assert!(matches!(
        ipc::recv(&mut reader),
        Ok(Some(Msg::Reply {
            id: Some(9),
            result: Ok(_)
        }))
    ));
    assert_eq!(
        handed(&rx),
        Some(View::Detail {
            account: "ale@acme.com".to_string(),
            id: "18f2".to_string(),
        })
    );

    // Something the window does not answer is refused, and the connection
    // stays up for whatever comes next.
    ipc::send(
        &mut stream,
        &Msg::Call {
            id: 10,
            tool: "grido_cell_set".to_string(),
            args: serde_json::json!({}),
        },
    )
    .expect("sent");
    assert!(matches!(
        ipc::recv(&mut reader),
        Ok(Some(Msg::Reply {
            id: Some(10),
            result: Err(_)
        }))
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A line that is not an op at all is ignored rather than answered, and the
/// window does not fall over reading it.
#[test]
fn a_line_that_is_not_an_op_moves_nothing() {
    let dir = scratch("garbage");
    let socket = dir.join("enclave-post.sock");
    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };

    let mut stream = UnixStream::connect(&socket).expect("the window answers");
    writeln!(stream, "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}}")
        .expect("sent");
    writeln!(stream, "not json at all").expect("sent");
    // The real errand, behind the noise.
    ipc::send(
        &mut stream,
        &Msg::View {
            id: Some(1),
            account: Some("ed@acme.com".to_string()),
            message: None,
        },
    )
    .expect("sent");

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) > 0 {
            break;
        }
    }
    assert!(ipc::is_op(&line), "the answer is an op: {line}");
    assert_eq!(handed(&rx), Some(inbox("ed@acme.com")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// The command line, the role, the view and the two fields on the socket are
/// one road with no forks in it.
#[test]
fn the_command_line_and_the_wire_agree_on_the_view() {
    for (args, view) in [
        (vec!["post"], View::Accounts),
        (vec!["post", "list", "ed@acme.com"], inbox("ed@acme.com")),
        (
            vec!["post", "read", "ed@acme.com", "18f3"],
            View::Detail {
                account: "ed@acme.com".to_string(),
                id: "18f3".to_string(),
            },
        ),
    ] {
        // What a person types asks for the window; it never becomes one.
        assert_eq!(
            role::of(&args),
            Role::PostOpen { view: view.clone() },
            "{args:?}"
        );
        let (account, message) = posthost::parts(&view);
        assert_eq!(posthost::view_of(account, message), view);
    }

    // A message with no account to read it from is not a message: the window
    // shows the accounts rather than nothing at all.
    assert_eq!(
        posthost::view_of(None, Some("18f3".to_string())),
        View::Accounts
    );
}

/// A process that will never be the window — the assistant answering
/// `post_show` — opens one the same two ways: the window that is up takes the
/// view, and when none is up a window process is started at it.
#[test]
fn a_windowless_process_hands_off_or_starts_the_window() {
    let dir = scratch("open");
    let socket = dir.join("enclave-post.sock");

    // Nothing is listening, so a window is started — at the view that was
    // asked for, and by a role rather than a command line.
    let started: RefCell<Vec<String>> = RefCell::new(Vec::new());
    posthost::open_at(&socket, &inbox("ed@acme.com"), |args| {
        started.borrow_mut().extend_from_slice(args);
        Ok(())
    })
    .expect("a window was started");
    assert_eq!(
        started.borrow().as_slice(),
        ["post", "--window", "list", "ed@acme.com"],
        "the window is started at the view"
    );
    assert_eq!(
        role::of(started.borrow().as_slice()),
        Role::PostWindow {
            view: inbox("ed@acme.com")
        },
        "the process that starts reads back the same view"
    );
    assert!(!socket.exists(), "opening a window binds nothing here");

    // Now one is up: it takes the view, and nothing is started beside it.
    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };
    posthost::open_at(&socket, &inbox("ale@acme.com"), |_| {
        panic!("a second window was started while one was up")
    })
    .expect("handed off");
    assert_eq!(handed(&rx), Some(inbox("ale@acme.com")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A view whose account or id could not be a word is refused before either
/// move, so `post_show` means the same thing whether a window is up or not —
/// never a different view, and never a terminal verb by accident.
#[test]
fn a_view_that_cannot_be_named_opens_nothing() {
    let dir = scratch("unnameable");
    let socket = dir.join("enclave-post.sock");

    for account in ["", "   ", "--page"] {
        assert!(
            posthost::open_at(&socket, &inbox(account), |_| panic!("started a window"))
                .is_err(),
            "{account:?} opened something"
        );
    }

    // The same answer with a window up: it is not asked either.
    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };
    assert!(
        posthost::open_at(&socket, &inbox("--page"), |_| panic!("started a window")).is_err()
    );
    assert_eq!(handed(&rx), None, "nothing was handed to the window");

    let _ = std::fs::remove_dir_all(&dir);
}

/// However a view is asked for, it is one road: the view names the words, the
/// words name the role, and the role is that same view again.
#[test]
fn the_words_a_window_is_started_with_are_the_view_itself() {
    for view in [
        View::Accounts,
        inbox("ed@acme.com"),
        View::Detail {
            account: "ed@acme.com".to_string(),
            id: "18f3a2c9b1".to_string(),
        },
    ] {
        let args = posthost::launch_args(&view);
        assert_eq!(args[0], "post", "{view:?}");
        assert_eq!(args[1], "--window", "{view:?}");
        assert_eq!(View::of_words(&args[2..]), Some(view.clone()), "{view:?}");
        assert_eq!(role::of(&args), Role::PostWindow { view: view.clone() });
        // And the socket says the same thing as the command line.
        let (account, message) = posthost::parts(&view);
        assert_eq!(posthost::view_of(account, message), view);
    }
}

/// The command a person types never becomes the window: it hands the view to
/// the window that is up, or starts one and goes. Either way the prompt comes
/// back at once, and the window it started is a process of its own.
#[test]
fn the_command_line_asks_for_the_window_and_never_becomes_one() {
    let dir = scratch("detach");
    let socket = dir.join("enclave-post.sock");

    // Whatever a person can type, the role is the one that hands off.
    for args in [
        vec!["post"],
        vec!["post", "list", "ed@acme.com"],
        vec!["post", "read", "ed@acme.com", "18f3"],
    ] {
        assert!(
            matches!(role::of(&args), Role::PostOpen { .. }),
            "{args:?} would have held the terminal"
        );
    }

    // Nothing is listening, so a window is started — and it is the started
    // process, not this one, that reads back the role which holds the loop.
    let started: RefCell<Vec<String>> = RefCell::new(Vec::new());
    posthost::open_at(&socket, &inbox("ed@acme.com"), |args| {
        started.borrow_mut().extend_from_slice(args);
        Ok(())
    })
    .expect("a window was started");
    assert_eq!(
        role::of(started.borrow().as_slice()),
        Role::PostWindow {
            view: inbox("ed@acme.com")
        },
        "the started process is the window"
    );
    assert!(!socket.exists(), "the command bound nothing of its own");

    // And with a window up, nothing is started at all.
    let Outcome::Run(rx) = posthost::start_at(&socket, &View::Accounts) else {
        panic!("the first launch is the window");
    };
    posthost::open_at(&socket, &inbox("ale@acme.com"), |_| {
        panic!("a second window was started while one was up")
    })
    .expect("handed off");
    assert_eq!(handed(&rx), Some(inbox("ale@acme.com")));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Post's door is its own, beside the app socket rather than on it — a mail
/// window must never be mistaken for the assistant's server.
#[test]
fn post_keeps_its_own_socket() {
    let post = posthost::socket_path();
    let app = enclave::mcp::proto::socket_path();
    assert_ne!(post, app);
    assert_eq!(post.parent(), app.parent());
    assert!(
        post.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("-post.sock")),
        "got {}",
        post.display()
    );
}
