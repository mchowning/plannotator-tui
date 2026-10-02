#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use plannotator_tui_schema::thread::messages;
use plannotator_tui_schema::{Fork, Kind, Thread, ThreadState, Turn};

use super::*;
use crate::store::Location;

fn fixture(tag: &str) -> (std::path::PathBuf, Location, Document, Store) {
    let root = std::env::temp_dir().join(format!("plannotator-handoff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let location = Location::for_file(&root.join("data"), "project", &root.join("plan.md"));
    let doc = Document::parse("one two three\n".to_owned());
    let store = Store::load(&location, &doc).expect("empty store");
    (root, location, doc, store)
}

fn fork() -> Fork {
    Fork {
        session_path: "/tmp/fork.jsonl".into(),
        origin_session: "main".into(),
        fork_point: "main.jsonl#e1".into(),
        model: "claude-bridge/claude-haiku-4-5".into(),
        thinking: "low".into(),
    }
}

/// Everything `render` saw, as the app renders it: every message body, in order.
fn transcript(view: &Store) -> (String, Vec<String>) {
    let text =
        view.annotations.iter().flat_map(messages).map(|m| m.body.to_owned()).collect::<Vec<_>>().join("|");
    (text, view.annotations.iter().map(|a| a.id.clone()).collect())
}

#[test]
fn a_send_renders_the_record_under_the_lock_and_commits_handoff_delivery_and_archive_together() {
    let (root, location, doc, mut store) = fixture("send");
    store.add(&doc, 0..3, "one".into(), Kind::Comment, "note".into()).expect("comment");
    let id = store.add_thread(&doc, 4..7, "two".into(), "why two?".into()).expect("thread");
    store.set_fork(&id, fork()).expect("fork");
    store.set_turn(&id, Turn::Running).expect("running");
    // The runner, in another process, writes after this store last read the record.
    let mut runner = Store::load(&location, &doc).expect("runner");
    runner.add_agent_reply(&id, &id, "two follows one".into()).expect("reply");
    // The person replied again and that turn is still running when they send.
    let queued = runner.add_user_reply(&id, "and three?".into()).expect("user reply");
    runner.set_turn(&id, Turn::Running).expect("second turn");

    let handoff = store.hand_off(&doc, "pi-session-1", "pi", transcript).expect("send");
    assert_eq!(handoff.owner, "pi-session-1");
    assert_eq!(
        handoff.body, "note|why two?|two follows one|and three?",
        "the reply that landed first is sent, and the thread ends with its unanswered message"
    );

    let reopened = Store::load(&location, &doc).expect("reopen");
    assert_eq!(reopened.len(), 0, "the review reopens empty");
    let mut disk = reopened.clone();
    assert_eq!(disk.handoff().expect("read"), Some(handoff));
    let archived = reopened.archived().iter().find(|a| a.id == id).expect("archived thread");
    let thread = Thread::of(archived).expect("readable").expect("still a thread");
    assert_eq!(thread.state, ThreadState::Historical);
    assert_eq!(thread.fork, None, "the fork link is dropped");
    assert_eq!(thread.turn, Turn::Interrupted, "an archived thread is never shown working");
    assert_eq!(messages(archived).len(), 3, "the messages are kept");

    assert!(
        runner.add_agent_reply(&id, &queued, "late".into()).is_err(),
        "a reply after the send is rejected"
    );
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_send_while_an_earlier_handoff_is_undelivered_writes_nothing() {
    let (root, location, doc, mut store) = fixture("pending");
    store.add(&doc, 0..3, "one".into(), Kind::Comment, "first".into()).expect("comment");
    store.hand_off(&doc, "pi-1", "pi", transcript).expect("first send");
    store.add(&doc, 4..7, "two".into(), Kind::Comment, "second".into()).expect("comment");
    let before = std::fs::read(&location.record).expect("record");

    let error = store.hand_off(&doc, "pi-1", "pi", transcript).expect_err("refused");
    assert!(error.to_string().contains("undelivered"), "{error:#}");
    assert_eq!(std::fs::read(&location.record).expect("record"), before);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_send_with_nothing_to_send_writes_nothing() {
    let (root, location, doc, mut store) = fixture("empty");
    store.add(&doc, 0..3, "one".into(), Kind::Comment, "note".into()).expect("comment");
    let before = std::fs::read(&location.record).expect("record");
    assert!(store.hand_off(&doc, "pi-1", "pi", |_| (String::new(), Vec::new())).is_err());
    assert_eq!(std::fs::read(&location.record).expect("record"), before);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_restored_thread_is_a_read_only_transcript() {
    let (root, _, doc, mut store) = fixture("restore");
    let id = store.add_thread(&doc, 4..7, "two".into(), "why two?".into()).expect("thread");
    store.hand_off(&doc, "pi-1", "pi", transcript).expect("send");
    store.ack_handoff(&store.clone().handoff().expect("read").expect("pending").id).expect("ack");
    assert_eq!(store.restore_archived(&doc, std::slice::from_ref(&id)).expect("restore"), 1);

    let entry = &store.threads().expect("threads")[0];
    assert_eq!(entry.thread.state, ThreadState::Historical);
    assert!(!entry.thread.needs_turn(entry.annotation), "no turn ever runs on it");
    assert!(store.add_user_reply(&id, "more?".into()).is_err());
    assert_eq!(store.thread_key(&id).expect("ctrl-t"), crate::store::ThreadKey::AlreadyThread);
    std::fs::remove_dir_all(root).expect("cleanup");
}
