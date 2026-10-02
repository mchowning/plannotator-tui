#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use plannotator_tui_schema::Kind;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent};

use crate::app::review_test_support::{draw, file_app, press};
use crate::app::{App, Focus, Mode};

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn footer(app: &mut App) -> String {
    draw(app, 160, 24).lines().last().expect("footer").trim().to_owned()
}

/// A review with a comment on "one" and a thread on "two"; the rail is focused.
fn notes_app(tag: &str) -> (std::path::PathBuf, App) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.add_quote_annotation("two", Kind::Comment, "B".into()).expect("comment");
    let thread = app.open.store.placed()[1].annotation.id.clone();
    app.open.store.thread_key(&thread).expect("thread");
    app.focus = Focus::Rail;
    app.status = None;
    (root, app)
}

#[test]
fn removing_a_rail_note_asks_first_and_y_removes_it() {
    let (_root, mut app) = notes_app("remove-yes");
    press(&mut app, 'x');
    assert!(matches!(app.mode, Mode::ConfirmRemove(_)));
    assert_eq!(app.open.store.len(), 2, "nothing removed before the answer");
    assert_eq!(footer(&mut app), "remove this note? y remove · n keep");
    press(&mut app, 'y');
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.open.store.len(), 1);
    assert_eq!(app.status.as_deref(), Some("annotation removed"));
}

#[test]
fn n_or_esc_keeps_the_note_and_del_asks_too() {
    let (_root, mut app) = notes_app("remove-no");
    press(&mut app, 'x');
    press(&mut app, 'n');
    assert_eq!(app.mode, Mode::Browse);
    key(&mut app, KeyCode::Delete);
    assert!(matches!(app.mode, Mode::ConfirmRemove(_)));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.open.store.len(), 2);
}

#[test]
fn a_thread_is_named_as_one_in_the_question() {
    let (_root, mut app) = notes_app("remove-thread");
    app.rail_cursor = 1;
    press(&mut app, 'x');
    assert_eq!(footer(&mut app), "remove this thread and its conversation? y remove · n keep");
}

#[test]
fn other_keys_do_nothing_while_asking() {
    let (_root, mut app) = notes_app("remove-swallow");
    press(&mut app, 'x');
    for ch in ['q', 'x', 'E', '?'] {
        press(&mut app, ch);
        assert!(matches!(app.mode, Mode::ConfirmRemove(_)), "{ch} ended the question");
    }
    assert!(!app.quit);
    assert_eq!(app.open.store.len(), 2);
}

#[test]
fn x_on_a_block_asks_with_the_count_and_y_removes_them_all() {
    let (_root, mut app, _) = file_app("remove-block");
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("two", Kind::Comment, "A".into()).expect("comment");
    app.add_quote_annotation("wo", Kind::Comment, "B".into()).expect("comment");
    let thread = app.open.store.placed()[0].annotation.id.clone();
    app.open.store.thread_key(&thread).expect("thread");
    app.selected = 2;
    app.status = None;
    press(&mut app, 'x');
    assert_eq!(footer(&mut app), "remove the 2 notes on this block, 1 of them a thread? y remove · n keep");
    press(&mut app, 'y');
    assert_eq!(app.open.store.len(), 0);
    assert_eq!(app.status.as_deref(), Some("removed 2 annotation(s) on block"));
}

#[test]
fn x_on_a_block_without_notes_does_not_ask() {
    let (_root, mut app, _) = file_app("remove-empty-block");
    app.add_quote_annotation("two", Kind::Comment, "A".into()).expect("comment");
    app.selected = 1;
    press(&mut app, 'x');
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.open.store.len(), 1);
}

#[test]
fn removing_the_last_rail_note_returns_focus_to_the_document() {
    let (_root, mut app) = notes_app("remove-last");
    press(&mut app, 'x');
    press(&mut app, 'y');
    assert_eq!(app.focus, Focus::Rail, "a note is left");
    press(&mut app, 'x');
    press(&mut app, 'y');
    assert_eq!(app.open.store.len(), 0);
    assert_eq!(app.focus, Focus::Document);
}
