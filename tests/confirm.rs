//! The confirmation broker: what is asked about, and what each answer means.
//!
//! The dialog itself cannot be exercised without a screen, so what is checked
//! here is everything behind it — the queue, the four ways a question ends, the
//! allowlist on disk, and the one line each way between the server and the
//! process that asks. `tests/headless.rs` drives that pipe end to end.

use std::time::Duration;

use enclave::confirm::{acting, summary, Answer, Broker, Decision, Request, DENIED};
use serde_json::json;

/// A scratch allowlist path nothing else will touch. Everything the tests write
/// lives under one directory, so clearing up after a run is one `rm`.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join("enclave-tests")
        .join(format!("confirm-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    dir.join("allowlist.toml")
}

/// Waits for the broker to have a question, so a test never races the thread
/// that is asking it.
fn question(broker: &Broker) -> enclave::confirm::Waiting {
    for _ in 0..200 {
        if let Some(w) = broker.front() {
            return w;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no question was ever asked");
}

#[test]
fn reading_never_asks() {
    let broker = Broker::new(scratch("reads"));
    for tool in [
        "grido_workbook_info",
        "grido_range_read",
        "grido_table_schema",
        "grido_table_find",
        "grido_find",
        "grido_stats",
        "enclave_status",
        "enclave_computers",
    ] {
        assert!(
            broker.decide(tool, &json!({})).is_ok(),
            "{tool} should run without asking"
        );
    }
    assert_eq!(broker.waiting(), 0);
}

#[test]
fn an_allowed_tool_runs_without_asking() {
    let file = scratch("allowed");
    std::fs::write(&file, "allow = [\"grido_cell_set\"]\n").expect("write");
    let broker = Broker::new(file);
    assert!(broker.allowed("grido_cell_set"));
    assert!(broker
        .decide("grido_cell_set", &json!({"cell": "A1", "value": 5}))
        .is_ok());
    assert_eq!(broker.waiting(), 0, "nothing should be queued");
}

#[test]
fn approving_lets_the_call_through() {
    let broker = std::sync::Arc::new(Broker::new(scratch("approve")));
    let asking = broker.clone();
    let call = std::thread::spawn(move || {
        asking.decide("grido_cell_set", &json!({"cell": "A1", "value": 5}))
    });

    let w = question(&broker);
    assert_eq!(w.tool, "grido_cell_set");
    assert!(
        w.summary.contains("set A1 to 5"),
        "the sentence should say what happens, got {:?}",
        w.summary
    );
    broker.resolve(w.id, Decision::Approve, false);

    assert!(call.join().expect("the call thread").is_ok());
    assert_eq!(broker.waiting(), 0);
    // One approval is one approval: the next call asks again.
    assert!(!broker.allowed("grido_cell_set"));
}

#[test]
fn denying_comes_back_as_denied() {
    let broker = std::sync::Arc::new(Broker::new(scratch("deny")));
    let asking = broker.clone();
    let call = std::thread::spawn(move || asking.decide("grido_rows_delete", &json!({"at": "4"})));

    let w = question(&broker);
    broker.resolve(w.id, Decision::Deny, false);

    let error = call.join().expect("the call thread").expect_err("a refusal");
    assert_eq!(error, DENIED);
    assert_eq!(broker.waiting(), 0);
}

/// Nobody at the keyboard is a no. The waiting call gives up on its own even
/// if no window is running to time it out.
#[test]
fn no_answer_is_a_refusal() {
    let broker = std::sync::Arc::new(Broker::with_timeout(
        scratch("timeout"),
        Duration::from_millis(50),
    ));
    let asking = broker.clone();
    let call = std::thread::spawn(move || asking.decide("enclave_send_file", &json!({})));

    let error = call.join().expect("the call thread").expect_err("a refusal");
    assert!(error.starts_with(DENIED), "got {error:?}");
    assert!(error.contains("no answer"), "should say why: {error:?}");
    assert_eq!(broker.waiting(), 0, "the question should be gone");
}

/// The server-side fuse, burning without a dialog anywhere. The thread that
/// asks the questions calls `expire` as it waits, which is what keeps a question
/// queued behind a long-lived dialog from outliving its countdown.
#[test]
fn the_countdown_refuses_it() {
    let broker = std::sync::Arc::new(Broker::with_timeout(
        scratch("expire"),
        Duration::from_millis(60),
    ));
    let asking = broker.clone();
    let call = std::thread::spawn(move || asking.decide("grido_sort", &json!({"range": "A1:B9"})));

    let w = question(&broker);
    assert!(w.left <= 1);
    std::thread::sleep(Duration::from_millis(90));
    broker.expire();

    let error = call.join().expect("the call thread").expect_err("a refusal");
    assert_eq!(error, DENIED);
    assert_eq!(broker.waiting(), 0);
}

#[test]
fn always_allow_is_written_down_and_remembered() {
    let file = scratch("always");
    let _ = std::fs::remove_file(&file);
    let broker = std::sync::Arc::new(Broker::new(file.clone()));

    let first = {
        let asking = broker.clone();
        std::thread::spawn(move || asking.decide("grido_cell_set", &json!({"cell": "A1"})))
    };
    let w = question(&broker);
    broker.resolve(w.id, Decision::Approve, true);
    assert!(first.join().expect("the call thread").is_ok());

    assert!(broker.allowed("grido_cell_set"));
    // The next one does not ask at all.
    assert!(broker.decide("grido_cell_set", &json!({})).is_ok());
    assert_eq!(broker.waiting(), 0);

    let written = std::fs::read_to_string(&file).expect("an allowlist file");
    assert!(written.contains("grido_cell_set"), "got {written:?}");
    // And a server started tomorrow reads the same answer.
    assert!(Broker::new(file).allowed("grido_cell_set"));
}

/// Saying "always" while more of the same tool is queued answers those too.
#[test]
fn always_allow_releases_what_is_already_queued() {
    let broker = std::sync::Arc::new(Broker::new(scratch("queued")));
    let calls: Vec<_> = (0..3)
        .map(|i| {
            let asking = broker.clone();
            std::thread::spawn(move || asking.decide("grido_cell_set", &json!({"cell": i})))
        })
        .collect();
    // Wait for all three to be queued before answering any.
    for _ in 0..200 {
        if broker.waiting() == 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(broker.waiting(), 3);

    let w = question(&broker);
    broker.resolve(w.id, Decision::Approve, true);
    for call in calls {
        assert!(call.join().expect("the call thread").is_ok());
    }
    assert_eq!(broker.waiting(), 0);
}

/// The acting set, tool by tool, against the whole advertised surface: if a
/// verb changes something it is asked about, and if it only looks it is not.
#[test]
fn every_advertised_tool_is_classified() {
    const READS: [&str; 6] = [
        "grido_workbook_info",
        "grido_range_read",
        "grido_table_schema",
        "grido_table_find",
        "grido_find",
        "grido_stats",
    ];
    let mut acted = 0;
    let mut read = 0;
    for tool in enclave::mcp::proto::definitions() {
        let name = tool["name"].as_str().expect("every tool has a name");
        if READS.contains(&name) || name == "enclave_status" || name == "enclave_computers" {
            assert!(!acting(name), "{name} only looks; it must not ask");
            read += 1;
        } else {
            assert!(acting(name), "{name} acts; it must be asked about");
            acted += 1;
        }
    }
    // The whole surface, not a subset of it: 20 grido verbs and 3 enclave ones.
    assert_eq!(read + acted, 23, "the tool count changed; check this list");
    assert_eq!(read, 8);
    assert!(acting("enclave_send_file"));
    // A grido verb added later is asked about until someone says otherwise.
    assert!(acting("grido_something_new"));
    // Nothing outside the product's own names is in the set at all.
    assert!(!acting("workbook_info"));
    assert!(!acting("rm"));
}

/// What the dialog process is handed: everything it draws, and no way to reach
/// back into the server.
#[test]
fn the_question_travels_as_one_line() {
    let broker = std::sync::Arc::new(Broker::new(scratch("wire")));
    let asking = broker.clone();
    let call = std::thread::spawn(move || {
        asking.decide("grido_cell_set", &json!({"cell": "A1", "value": 5}))
    });
    let w = question(&broker);

    let request = Request::about(&w);
    assert_eq!(request.tool, "grido_cell_set");
    assert_eq!(request.sentence, w.summary);
    assert!(request.args.contains("A1"), "got {:?}", request.args);
    assert_eq!(request.always_label, "Always allow grido_cell_set");
    assert!(request.seconds > 0);

    let line = request.encode();
    assert!(!line.contains('\n'), "one question is one line: {line}");
    assert_eq!(Request::parse(&line), Some(request));
    // Nothing that could be used to answer a different question.
    let sent: serde_json::Value = serde_json::from_str(&line).expect("json");
    assert!(sent.get("id").is_none(), "the dialog knows no ids: {line}");

    broker.resolve(w.id, Decision::Deny, false);
    let _ = call.join();
}

/// The answer, and every way of not giving one.
#[test]
fn only_a_clean_approval_is_an_approval() {
    let approve = Answer {
        decision: Decision::Approve,
        always_allow: true,
    };
    let line = approve.encode();
    assert!(!line.contains('\n'), "one answer is one line: {line}");
    assert_eq!(Answer::parse(&line), Some(approve));
    assert_eq!(
        Answer::parse(&Answer::deny().encode()),
        Some(Answer {
            decision: Decision::Deny,
            always_allow: false
        })
    );
    // A trailing newline is how it arrives off a pipe.
    assert_eq!(
        Answer::parse("{\"decision\":\"approve\"}\n"),
        Some(Answer {
            decision: Decision::Approve,
            always_allow: false
        }),
        "no checkbox means no checkbox, not a broken answer"
    );
    for nothing in [
        "",
        "\n",
        "yes",
        "{}",
        "{\"decision\":\"maybe\"}",
        "{\"decision\":true}",
        "{\"always_allow\":true}",
    ] {
        assert_eq!(
            Answer::parse(nothing),
            None,
            "{nothing:?} is not an answer, so it is a refusal"
        );
    }
}

#[test]
fn the_sentence_says_what_will_happen() {
    let send = summary(
        "enclave_send_file",
        &json!({"computer": "ale-mbp", "path": "/home/ed/q3.xlsx"}),
    );
    assert!(send.contains("q3.xlsx"), "got {send:?}");
    assert!(send.contains("ale-mbp"), "got {send:?}");

    let delete = summary("grido_rows_delete", &json!({"at": "4:9"}));
    assert!(delete.contains("Grido"), "got {delete:?}");
    assert!(delete.contains("4:9"), "got {delete:?}");
}
