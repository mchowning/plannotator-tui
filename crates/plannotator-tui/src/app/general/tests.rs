#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use plannotator_tui_schema::Kind;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::review_test_support::{draw, file_app, press, reply_app};
use crate::app::{App, Focus, Mode};

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn type_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, c));
}

fn write_general(app: &mut App, text: &str) {
    press(app, 'C');
    type_text(app, text);
    key(app, KeyCode::Enter);
}

#[test]
fn capital_c_writes_the_general_comment_and_reopens_it_to_edit() {
    let (root, mut app, _) = file_app("write");
    write_general(&mut app, "too long");
    press(&mut app, 'C');
    assert_eq!(app.compose.value(), "too long", "the existing comment, ready to edit");
    type_text(&mut app, " overall");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.open.store.len(), 1, "still one general comment");
    assert_eq!(app.open.store.general().map(|a| a.body.as_str()), Some("too long overall"));
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn escape_leaves_the_general_comment_as_it_was() {
    let (root, mut app, _) = file_app("cancel");
    press(&mut app, 'C');
    type_text(&mut app, "never mind");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert!(app.open.store.general().is_none());
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn the_general_comment_is_sent_ahead_of_the_line_notes_and_then_counts_as_sent() {
    let (root, mut app, delivery) = file_app("send");
    app.add_quote_annotation("two", Kind::Comment, "which two?".into()).expect("note");
    write_general(&mut app, "overall fine");
    assert_eq!(app.send_count(), 2);
    press(&mut app, 'E');
    let sent = delivery.calls.borrow()[0].clone();
    let general = sent.find("## General feedback\n> overall fine").expect("general sent");
    assert!(general < sent.find("## Annotation 1").expect("note sent"), "{sent}");
    assert_eq!(app.send_count(), 0);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_reply_review_with_only_a_general_comment_sends_it() {
    let (root, mut app, delivery) = reply_app("reply");
    write_general(&mut app, "looks right");
    press(&mut app, 'S');
    assert!(delivery.calls.borrow()[0].contains("## General feedback\n> looks right"));
    assert!(app.quit);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn the_rail_shows_the_general_comment_first_and_x_removes_it() {
    let (root, mut app, _) = file_app("rail");
    app.add_quote_annotation("one", Kind::Comment, "first note".into()).expect("note");
    write_general(&mut app, "overall fine");
    let screen = draw(&mut app, 100, 24);
    let general_row = screen.lines().position(|l| l.contains("📄")).expect("general card");
    let note_row = screen.lines().position(|l| l.contains("first note")).expect("note card");
    assert!(general_row < note_row, "{screen}");

    app.focus = Focus::Rail;
    app.rail_cursor = 0;
    press(&mut app, 'x');
    press(&mut app, 'y');
    assert!(app.open.store.general().is_none());
    assert_eq!(app.open.store.len(), 1, "the line note stays");
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_general_comment_cannot_become_a_thread() {
    let (root, mut app, _) = file_app("thread");
    app.attach_agent_session("session-1".into());
    write_general(&mut app, "overall");
    app.focus = Focus::Rail;
    app.rail_cursor = 0;
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL))).expect("ctrl-t");
    assert!(app.open.store.threads().expect("threads").is_empty());
    std::fs::remove_dir_all(root).expect("cleanup");
}
