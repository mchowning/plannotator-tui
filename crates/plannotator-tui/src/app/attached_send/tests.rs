#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use std::path::PathBuf;

use plannotator_tui_schema::{Kind, Provenance, Turn};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent};
use serde_json::Value;

use crate::app::review_test_support::{draw, file_app, press, reopen};
use crate::app::{App, Mode};
use crate::store::Location;

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn doc_path(app: &App) -> PathBuf {
    let Provenance::File { path } = &app.open.source.provenance else { panic!("file") };
    path.clone()
}

fn record_path(app: &App) -> PathBuf {
    Location::for_file(&app.data_dir, &app.project, &doc_path(app)).record
}

fn record(app: &App) -> Value {
    serde_json::from_str(&std::fs::read_to_string(record_path(app)).expect("record")).expect("json")
}

/// Attached, with two regular comments, an answered thread on "two", and a thread on a
/// paragraph that was then deleted from the file.
fn review(tag: &str) -> (PathBuf, App, String) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi-session-1".into());
    let path = doc_path(&app);
    std::fs::write(&path, "# Plan\n\none\n\ntwo\n\nthree\n\nfour\n").expect("doc");
    reopen(&mut app);
    app.add_quote_annotation("one", Kind::Comment, "note A".into()).expect("A");
    app.add_quote_annotation("three", Kind::Comment, "note C".into()).expect("C");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("thread");
    app.add_quote_annotation("four", Kind::Comment, "Where did four go?".into()).expect("detached");
    let ids: Vec<String> = app.open.store.placed().iter().map(|p| p.annotation.id.clone()).collect();
    let (two, four) = (ids[1].clone(), ids[3].clone());
    app.open.store.thread_key(&two).expect("thread");
    app.open.store.thread_key(&four).expect("thread");
    app.open.store.add_agent_reply(&two, &two, "Two follows one.\nThen three.".into()).expect("reply");
    std::fs::write(&path, "# Plan\n\none\n\ntwo\n\nthree\n").expect("delete four");
    press(&mut app, 'r');
    (root, app, two)
}

#[test]
fn the_sent_body_has_every_comment_as_today_and_every_thread_as_a_transcript() {
    let (_root, mut app, _) = review("golden");
    press(&mut app, 'q');
    press(&mut app, 'y');
    let body = record(&app)["handoff"]["body"].as_str().expect("handoff body").to_owned();
    assert_eq!(
        body,
        "# Annotations on a.md\n\n\
         ## Annotation 1 (line 3)\nComment on: \"one\"\n> note A\n\n\
         ## Annotation 2 (line 5)\nThread on: \"two\"\n- **user:** Why two?\n- **agent:** Two follows one.\n  Then three.\n\n\
         ## Annotation 3 (line 7)\nComment on: \"three\"\n> note C\n\n\
         ## Annotation 4 (passage since changed)\nThread on: \"four\"\n- **user:** Where did four go?\n\n"
    );
}

#[test]
fn q_in_an_attached_review_asks_to_send_and_esc_stays() {
    let (_root, mut app, _) = review("prompt");
    press(&mut app, 'q');
    assert_eq!(app.mode, Mode::ConfirmQuit);
    let screen = draw(&mut app, 160, 45);
    assert!(screen.contains("send this review to pi? y send · n keep for later · esc stay"), "{screen}");
    assert!(!screen.contains("aborted"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert!(!app.quit);
}

#[test]
fn e_opens_the_same_prompt_in_an_attached_review() {
    let (_root, mut app, _) = review("prompt-e");
    press(&mut app, 'E');
    assert_eq!(app.mode, Mode::ConfirmQuit);
    assert!(record(&app).get("handoff").is_none(), "nothing is sent until y");
}

#[test]
fn a_running_turn_is_named_in_the_prompt() {
    let (_root, mut app, two) = review("prompt-running");
    app.open.store.set_turn(&two, Turn::Running).expect("running");
    press(&mut app, 'q');
    assert!(draw(&mut app, 160, 45).contains("running turns will be aborted"));
}

#[test]
fn n_quits_and_leaves_the_record_byte_identical() {
    let (_root, mut app, two) = review("prompt-n");
    app.open.store.set_turn(&two, Turn::Running).expect("running");
    let before = std::fs::read(record_path(&app)).expect("record");
    press(&mut app, 'q');
    press(&mut app, 'n');
    assert!(app.quit);
    assert_eq!(std::fs::read(record_path(&app)).expect("record"), before);
}

#[test]
fn y_sends_to_the_owning_session_quits_and_the_document_reopens_empty() {
    let (_root, mut app, two) = review("prompt-y");
    press(&mut app, 'q');
    press(&mut app, 'y');
    assert!(app.quit);

    let data = record(&app);
    assert_eq!(data["handoff"]["owner"], "pi-session-1");
    assert_eq!(data["annotations"].as_array().map(Vec::len), Some(0), "{data:#}");
    let thread = data["archived"]
        .as_array()
        .expect("archived")
        .iter()
        .find(|a| a["id"] == two.as_str())
        .expect("thread");
    assert_eq!(thread["plannotator_tui_thread"]["state"], "historical");
    assert!(thread["plannotator_tui_thread"].get("fork").is_none());

    reopen(&mut app);
    assert_eq!(app.open.store.len(), 0, "reopening starts an empty review");
    let history =
        std::fs::read_to_string(app.data_dir.join("feedback").join(&app.project).join("index.jsonl"))
            .expect("feedback history");
    let newest: Value = serde_json::from_str(history.lines().last().expect("a line")).expect("json");
    assert!(newest.to_string().contains("Two follows one."), "the archive holds the transcript: {newest}");
}

#[test]
fn a_send_refused_by_an_undelivered_handoff_stays_open_and_says_why() {
    let (_root, mut app, _) = review("prompt-waiting");
    let mut data = record(&app);
    data["handoff"] = serde_json::json!({"id": "h1", "owner": "pi-old", "body": "x", "created_at": "t"});
    std::fs::write(record_path(&app), data.to_string()).expect("write");
    press(&mut app, 'q');
    press(&mut app, 'y');
    assert!(!app.quit);
    assert!(app.status.as_deref().is_some_and(|s| s.contains("pi-old")), "{:?}", app.status);
}
