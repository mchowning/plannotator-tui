#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use std::path::PathBuf;

use plannotator_tui_schema::{Kind, Turn};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;

use crate::app::review_test_support::{file_app, folder_app, press};
use crate::app::{App, Focus, Mode};
use crate::store::Location;

fn ctrl_r(app: &mut App) {
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))).expect("ctrl-r");
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn type_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, c));
}

fn record_path(app: &App) -> PathBuf {
    let plannotator_tui_schema::Provenance::File { path } = &app.open.source.provenance else {
        panic!("a file review")
    };
    Location::for_file(&app.data_dir, &app.project, path).record
}

fn record(app: &App) -> Value {
    serde_json::from_str(&std::fs::read_to_string(record_path(app)).expect("record")).expect("json")
}

/// Compose a comment on the selected block, ending with `finish`.
fn compose(app: &mut App, text: &str, finish: impl FnOnce(&mut App)) {
    press(app, 'c');
    assert_eq!(app.mode, Mode::Compose);
    type_text(app, text);
    finish(app);
}

#[test]
fn ctrl_r_in_compose_saves_a_thread_when_an_agent_session_is_attached() {
    let (_root, mut app, _) = file_app("thread-compose");
    app.attach_agent_session("pi-session-1".into());
    compose(&mut app, "Is Friday firm?", ctrl_r);

    assert_eq!(app.mode, Mode::Browse);
    let annotations = record(&app)["annotations"].clone();
    assert_eq!(annotations.as_array().map(Vec::len), Some(1));
    assert_eq!(annotations[0]["body"], "Is Friday firm?");
    assert_eq!(annotations[0]["plannotator_tui_thread"]["state"], "live");
    assert_eq!(app.status.as_deref(), Some("thread started"));
}

#[test]
fn enter_in_compose_still_saves_a_regular_comment() {
    let (_root, mut app, _) = file_app("thread-enter");
    app.attach_agent_session("pi-session-1".into());
    compose(&mut app, "Plain note", |app| key(app, KeyCode::Enter));

    let annotation = record(&app)["annotations"][0].clone();
    assert_eq!(annotation["body"], "Plain note");
    assert!(annotation.get("plannotator_tui_thread").is_none());
}

#[test]
fn without_an_agent_session_ctrl_r_says_so_and_writes_nothing() {
    let (_root, mut app, _) = file_app("thread-unattached");
    compose(&mut app, "Is Friday firm?", ctrl_r);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert_eq!(app.mode, Mode::Compose, "the typed text is kept");
    assert!(!record_path(&app).exists(), "nothing was written");
}

#[test]
fn a_folder_review_is_never_attached() {
    let (_root, mut app, _) = folder_app("thread-folder");
    app.attach_agent_session("pi-session-1".into());
    app.focus = Focus::Document;
    compose(&mut app, "Is Friday firm?", ctrl_r);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert!(!record_path(&app).exists());
}

#[test]
fn ctrl_r_on_a_rail_comment_makes_it_a_thread_and_keeps_its_body() {
    let (_root, mut app, _) = file_app("thread-rail");
    app.attach_agent_session("pi-session-1".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    ctrl_r(&mut app);

    let annotation = record(&app)["annotations"][0].clone();
    assert_eq!(annotation["body"], "Why two?");
    assert_eq!(annotation["plannotator_tui_thread"]["state"], "live");
    assert_eq!(app.status.as_deref(), Some("thread started"));

    ctrl_r(&mut app);
    assert_eq!(app.status.as_deref(), Some("already a thread"));
}

#[test]
fn ctrl_r_on_a_failed_thread_retries_it() {
    let (_root, mut app, _) = file_app("thread-retry");
    app.attach_agent_session("pi-session-1".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    ctrl_r(&mut app);
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open
        .store
        .set_turn(&id, Turn::Failed { detail: "rate limited (429)".into(), retryable: true })
        .expect("fail");

    ctrl_r(&mut app);
    assert_eq!(app.status.as_deref(), Some("retrying"));
    assert_eq!(record(&app)["annotations"][0]["plannotator_tui_thread"]["turn"]["status"], "idle");
}

#[test]
fn ctrl_r_on_the_rail_without_an_agent_session_writes_nothing() {
    let (_root, mut app, _) = file_app("thread-rail-unattached");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let before = std::fs::read(record_path(&app)).expect("record");
    app.focus = Focus::Rail;
    ctrl_r(&mut app);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert_eq!(std::fs::read(record_path(&app)).expect("record"), before);
}
