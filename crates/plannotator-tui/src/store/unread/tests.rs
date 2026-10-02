#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use std::path::PathBuf;

use plannotator_tui_schema::{Kind, Turn, is_unread};

use crate::doc::Document;
use crate::store::{Location, Store};

fn fixture(tag: &str) -> (PathBuf, Location, Document, Store) {
    let root = std::env::temp_dir().join(format!("plannotator-unread-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let location = Location::for_file(&root.join("data"), "project", &root.join("plan.md"));
    let doc = Document::parse("one two three\n".to_owned());
    let store = Store::load(&location, &doc).expect("empty store");
    (root, location, doc, store)
}

fn unread(store: &Store, id: &str) -> bool {
    store.annotations.iter().find(|a| a.id == id).is_some_and(is_unread)
}

#[test]
fn a_marked_comment_stays_unread_across_a_reload_until_marked_read() {
    let (root, location, doc, mut store) = fixture("comment");
    store.add(&doc, 0..3, "one".into(), Kind::Comment, "later".into()).expect("comment");
    let id = store.annotations[0].id.clone();
    assert!(!unread(&store, &id), "a new comment is read");
    store.record_delivery("agent", std::slice::from_ref(&id)).expect("send");
    store.set_unread(&id, true).expect("mark");
    let mut reloaded = Store::load(&location, &doc).expect("reload");
    assert!(unread(&reloaded, &id));
    assert!(!reloaded.is_pending(&reloaded.annotations[0]), "marking is not an edit to send");
    reloaded.set_unread(&id, false).expect("read");
    assert!(!unread(&reloaded, &id));
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_turn_ending_marks_its_thread_unread_and_starting_one_does_not() {
    let (root, _, doc, mut store) = fixture("turns");
    let id = store.add_thread(&doc, 0..3, "one".into(), "why?".into()).expect("thread");
    assert!(!unread(&store, &id), "your own message is read");

    store.set_turn(&id, Turn::Running).expect("running");
    assert!(!unread(&store, &id), "a running turn has nothing to read yet");
    store.add_agent_reply(&id, &id, "because".into()).expect("answer");
    assert!(unread(&store, &id), "an answer is unread");

    for ended in [Turn::Failed { detail: "rate limited (429)".into(), retryable: true }, Turn::Interrupted] {
        store.set_unread(&id, false).expect("read");
        store.set_turn(&id, ended).expect("end");
        assert!(unread(&store, &id), "a turn that stopped needs you too");
    }

    store.set_unread(&id, false).expect("read");
    store.add_user_reply(&id, "and?".into()).expect("reply");
    store.add_note(&id, "main: rename it".into()).expect("note");
    assert!(!unread(&store, &id), "nothing you write makes it unread");
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn archiving_clears_unread() {
    let (root, _, doc, mut store) = fixture("archive");
    let id = store.add_thread(&doc, 0..3, "one".into(), "why?".into()).expect("thread");
    store.add_agent_reply(&id, &id, "because".into()).expect("answer");
    store.record_delivery("agent", std::slice::from_ref(&id)).expect("send");
    store.archive_sent().expect("finish");
    assert!(!store.archived().iter().any(is_unread));
    std::fs::remove_dir_all(root).expect("cleanup");
}
