#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use std::path::PathBuf;

use plannotator_tui_schema::{Kind, Turn};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;

use crate::app::review_test_support::{file_app, folder_app, press};
use crate::app::{App, Focus, Mode};
use crate::store::Location;

fn ctrl_t(app: &mut App) {
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL))).expect("ctrl-t");
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
fn ctrl_t_in_compose_saves_a_thread_when_an_agent_session_is_attached() {
    let (_root, mut app, _) = file_app("thread-compose");
    app.attach_agent_session("pi-session-1".into());
    compose(&mut app, "Is Friday firm?", ctrl_t);

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
fn without_an_agent_session_ctrl_t_says_so_and_writes_nothing() {
    let (_root, mut app, _) = file_app("thread-unattached");
    compose(&mut app, "Is Friday firm?", ctrl_t);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert_eq!(app.mode, Mode::Compose, "the typed text is kept");
    assert!(!record_path(&app).exists(), "nothing was written");
}

#[test]
fn a_folder_review_is_never_attached() {
    let (_root, mut app, _) = folder_app("thread-folder");
    app.attach_agent_session("pi-session-1".into());
    app.focus = Focus::Document;
    compose(&mut app, "Is Friday firm?", ctrl_t);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert!(!record_path(&app).exists());
}

#[test]
fn ctrl_t_on_a_rail_comment_makes_it_a_thread_and_keeps_its_body() {
    let (_root, mut app, _) = file_app("thread-rail");
    app.attach_agent_session("pi-session-1".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    ctrl_t(&mut app);

    let annotation = record(&app)["annotations"][0].clone();
    assert_eq!(annotation["body"], "Why two?");
    assert_eq!(annotation["plannotator_tui_thread"]["state"], "live");
    assert_eq!(app.status.as_deref(), Some("thread started"));

    ctrl_t(&mut app);
    assert_eq!(app.status.as_deref(), Some("already a thread"));
}

#[test]
fn ctrl_t_while_editing_a_comment_saves_the_edit_as_a_thread() {
    let (_root, mut app, _) = file_app("thread-edit");
    app.attach_agent_session("pi-session-1".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Edit(_)));
    type_text(&mut app, " Not three?");
    ctrl_t(&mut app);

    assert_eq!(app.mode, Mode::Browse);
    let annotation = record(&app)["annotations"][0].clone();
    assert_eq!(annotation["body"], "Why two? Not three?");
    assert_eq!(annotation["plannotator_tui_thread"]["state"], "live");
    assert_eq!(app.status.as_deref(), Some("thread started"));
}

#[test]
fn ctrl_t_while_editing_without_an_agent_session_keeps_the_box_open() {
    let (_root, mut app, _) = file_app("thread-edit-unattached");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let before = std::fs::read(record_path(&app)).expect("record");
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, " Not three?");
    ctrl_t(&mut app);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert!(matches!(app.mode, Mode::Edit(_)), "the typed text is kept");
    assert_eq!(std::fs::read(record_path(&app)).expect("record"), before);
}

#[test]
fn ctrl_t_on_a_failed_thread_retries_it() {
    let (_root, mut app, _) = file_app("thread-retry");
    app.attach_agent_session("pi-session-1".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    ctrl_t(&mut app);
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open
        .store
        .set_turn(&id, Turn::Failed { detail: "rate limited (429)".into(), retryable: true })
        .expect("fail");

    ctrl_t(&mut app);
    assert_eq!(app.status.as_deref(), Some("retrying"));
    assert_eq!(record(&app)["annotations"][0]["plannotator_tui_thread"]["turn"]["status"], "idle");
}

#[test]
fn ctrl_t_on_the_rail_without_an_agent_session_writes_nothing() {
    let (_root, mut app, _) = file_app("thread-rail-unattached");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let before = std::fs::read(record_path(&app)).expect("record");
    app.focus = Focus::Rail;
    ctrl_t(&mut app);

    assert_eq!(app.status.as_deref(), Some("no agent session attached"));
    assert_eq!(std::fs::read(record_path(&app)).expect("record"), before);
}

// ----- resolve (FR32–FR34) --------------------------------------------------------------

fn ctrl_r(app: &mut App) {
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))).expect("ctrl-r");
}

fn resolved(app: &App) -> bool {
    record(app)["annotations"][0]["plannotator_tui_thread"]["resolved"] == true
}

/// An attached review with an answered thread on "two" whose first message is three
/// lines; the rail is focused on it.
fn answered_app(tag: &str) -> (PathBuf, App, String) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?\nsecond line\nthird line".into())
        .expect("comment");
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open.store.thread_key(&id).expect("thread");
    app.open.store.add_agent_reply(&id, &id, "Two follows one.".into()).expect("reply");
    app.focus = Focus::Rail;
    (root, app, id)
}

#[test]
fn ctrl_r_on_a_rail_thread_resolves_it_and_again_unresolves_it() {
    let (_root, mut app, _) = answered_app("resolve-rail");
    ctrl_r(&mut app);
    assert!(resolved(&app));
    assert_eq!(app.status.as_deref(), Some("thread resolved"));
    ctrl_r(&mut app);
    assert!(!resolved(&app));
    assert_eq!(app.status.as_deref(), Some("thread unresolved"));
}

#[test]
fn a_resolved_box_is_titled_resolved_and_shows_one_row_of_the_first_message() {
    let (_root, mut app, _) = answered_app("resolve-box");
    ctrl_r(&mut app);
    let screen = crate::app::review_test_support::draw(&mut app, 160, 45);
    assert!(screen.contains("· thread · resolved"), "{screen}");
    assert!(screen.contains("Why two?…"), "one row, marked as cut short\n{screen}");
    assert!(!screen.contains("second line"), "collapsed to one row\n{screen}");
    assert!(!screen.contains("Two follows one."), "the latest message is hidden\n{screen}");

    ctrl_r(&mut app);
    let screen = crate::app::review_test_support::draw(&mut app, 160, 45);
    assert!(screen.contains("Two follows one.") && !screen.contains("· resolved"), "{screen}");
}

#[test]
fn ctrl_r_in_the_thread_panel_toggles_resolved_and_r_is_still_text() {
    let (_root, mut app, id) = answered_app("resolve-panel");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Thread(id.clone()));
    press(&mut app, 'r');
    ctrl_r(&mut app);
    assert!(resolved(&app));
    assert_eq!(app.mode, Mode::Thread(id), "the panel stays open");
    let screen = crate::app::review_test_support::draw(&mut app, 160, 45);
    assert!(screen.contains("thread · resolved"), "{screen}");
    key(&mut app, KeyCode::Enter);
    let replies = record(&app)["annotations"][0]["replies"].clone();
    assert_eq!(replies.as_array().and_then(|r| r.last()).map(|r| r["body"].clone()), Some("r".into()));
}

#[test]
fn resolving_needs_no_agent_session() {
    let (_root, mut app, _) = answered_app("resolve-unattached");
    app.agent_session = None;
    ctrl_r(&mut app);
    assert!(resolved(&app));
}

#[test]
fn ctrl_r_on_a_regular_comment_or_a_historical_thread_writes_nothing() {
    let (_root, mut app, _) = file_app("resolve-comment");
    app.add_quote_annotation("two", Kind::Comment, "plain".into()).expect("comment");
    app.focus = Focus::Rail;
    let before = std::fs::read(record_path(&app)).expect("record");
    ctrl_r(&mut app);
    assert_eq!(std::fs::read(record_path(&app)).expect("record"), before);
    assert_eq!(app.status.as_deref(), Some("not a thread"));

    let (_root, mut app, _) = answered_app("resolve-historical");
    let mut data = record(&app);
    data["annotations"][0]["plannotator_tui_thread"]["state"] = "historical".into();
    std::fs::write(record_path(&app), data.to_string()).expect("write");
    crate::app::review_test_support::reopen(&mut app);
    app.focus = Focus::Rail;
    ctrl_r(&mut app);
    assert!(!resolved(&app));
    assert_eq!(app.status.as_deref(), Some("read-only: this thread was already sent"));
}
