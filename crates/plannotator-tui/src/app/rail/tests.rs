#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use plannotator_tui_schema::{Kind, Provenance, Turn};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::style::Modifier;

use crate::app::review_test_support::{draw, file_app, press};
use crate::app::{App, Focus};

/// A thread on "two", attached.
fn thread_app(tag: &str) -> (std::path::PathBuf, App, String) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open.store.thread_key(&id).expect("thread");
    (root, app, id)
}

fn rail_text(app: &mut App) -> String {
    draw(app, 160, 45)
}

#[test]
fn a_thread_box_shows_its_latest_message() {
    let (_root, mut app, id) = thread_app("rail-latest");
    app.open.store.add_agent_reply(&id, &id, "Two follows one.".into()).expect("reply");
    let screen = rail_text(&mut app);
    assert!(screen.contains("Two follows one."), "{screen}");
    assert!(!screen.contains("Why two?"), "the box shows the latest message, not the first\n{screen}");
}

#[test]
fn a_running_turn_shows_working() {
    let (_root, mut app, id) = thread_app("rail-working");
    app.open.store.set_turn(&id, Turn::Running).expect("turn");
    assert!(rail_text(&mut app).contains("working…"));
}

#[test]
fn failed_and_interrupted_turns_show_their_status_line() {
    let (_root, mut app, id) = thread_app("rail-failed");
    app.open
        .store
        .set_turn(&id, Turn::Failed { detail: "rate limited (429)".into(), retryable: true })
        .expect("turn");
    let screen = rail_text(&mut app);
    assert!(screen.contains("turn failed: rate limited (429)"), "{screen}");
    assert!(!screen.contains("won't help"));

    app.open
        .store
        .set_turn(&id, Turn::Failed { detail: "context too long".into(), retryable: false })
        .expect("turn");
    assert!(rail_text(&mut app).contains("retrying won't help"));

    app.open.store.set_turn(&id, Turn::Interrupted).expect("turn");
    assert!(rail_text(&mut app).contains("turn interrupted"));
}

fn buffer(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

/// Where `text` starts on screen, and whether every one of its cells is struck through.
fn struck(buffer: &Buffer, text: &str) -> Option<bool> {
    let width = buffer.area.width;
    (0..buffer.area.height).find_map(|y| {
        let row: String =
            (0..width).filter_map(|x| buffer.cell((x, y))).map(ratatui::buffer::Cell::symbol).collect();
        let start = row.find(text)?;
        let x0 = row[..start].chars().count() as u16;
        Some(
            (0..text.chars().count() as u16).all(|dx| {
                buffer.cell((x0 + dx, y)).is_some_and(|c| c.modifier.contains(Modifier::CROSSED_OUT))
            }),
        )
    })
}

#[test]
fn a_thread_whose_passage_is_deleted_moves_to_a_detached_group_and_stays_selectable() {
    let (root, mut app, id) = thread_app("rail-detached");
    app.add_quote_annotation("three", Kind::Comment, "A plain note".into()).expect("comment");
    let Provenance::File { path } = app.open.source.provenance.clone() else { panic!("file") };
    std::fs::write(&path, "# Plan\n\none\n\nfour\n").expect("edit the file");
    press(&mut app, 'r');

    let screen = buffer(&mut app);
    let text = draw(&mut app, 160, 45);
    assert!(text.contains("detached"), "{text}");
    assert_eq!(struck(&screen, "two"), Some(true), "the old quote is struck through\n{text}");
    assert!(!text.contains("A plain note"), "a detached regular comment is not in the rail\n{text}");

    app.focus = Focus::Rail;
    app.handle_event(&Event::Key(KeyEvent::from(KeyCode::Home))).expect("key");
    assert_eq!(app.rail_selected_id().as_deref(), Some(id.as_str()));
    let _ = root;
}

#[test]
fn a_regular_comment_box_is_unchanged() {
    let (_root, mut app, _) = file_app("rail-regular");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let screen = rail_text(&mut app);
    assert!(
        screen.contains("Why two?") && !screen.contains("working…") && !screen.contains("thread"),
        "{screen}"
    );
}

#[test]
fn a_thread_box_previews_a_long_reply_in_a_few_lines_and_keeps_its_line_breaks() {
    let (_root, mut app, id) = thread_app("rail-preview");
    let long = (1..=12).map(|i| format!("point {i}")).collect::<Vec<_>>().join("\n");
    app.open.store.add_agent_reply(&id, &id, long).expect("reply");
    let screen = rail_text(&mut app);
    assert!(screen.contains("agent: point 1") && screen.contains("point 4"), "{screen}");
    assert!(!screen.contains("point 5"), "the rest is in the panel\n{screen}");
    assert!(screen.contains('…'), "{screen}");
}

/// Whether every drawn cell of the document, and of the first rail card, is dimmed.
fn dimmed(app: &mut App) -> (bool, bool) {
    let buffer = buffer(app);
    let all_dim = |rect: ratatui::layout::Rect| {
        rect.positions()
            .filter_map(|p| buffer.cell(p))
            .filter(|c| c.symbol() != " ")
            .all(|c| c.modifier.contains(Modifier::DIM))
    };
    let card = app.geometry.bubbles.first().expect("a card").0;
    (all_dim(app.geometry.doc), all_dim(card))
}

#[test]
fn the_pane_without_focus_is_dimmed() {
    let (_root, mut app, _) = file_app("rail-dim");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    assert_eq!(dimmed(&mut app), (false, true), "the document has focus: the rail is dimmed");
    app.handle_event(&Event::Key(KeyEvent::from(KeyCode::Tab))).expect("tab");
    assert_eq!(app.focus, Focus::Rail);
    assert_eq!(dimmed(&mut app), (true, false), "the rail has focus: the document is dimmed");
}

/// The note Tab lands on, in `file_app` with a comment on each of `quotes` (its body is
/// its quote), after `j` is pressed `down` times from the heading: 1 is "one", 2 "two",
/// 3 "three".
fn tab_lands_on(tag: &str, quotes: &[&str], down: usize) -> String {
    let (_root, mut app, _) = file_app(tag);
    for quote in quotes {
        app.add_quote_annotation(quote, Kind::Comment, (*quote).into()).expect("comment");
    }
    rail_text(&mut app);
    (0..down).for_each(|_| press(&mut app, 'j'));
    app.handle_event(&Event::Key(KeyEvent::from(KeyCode::Tab))).expect("tab");
    assert_eq!(app.focus, Focus::Rail);
    app.rail()[app.rail_cursor].annotation.body.clone()
}

#[test]
fn tab_goes_to_the_note_on_the_selected_block() {
    assert_eq!(tab_lands_on("tab-on", &["one", "three"], 3), "three");
}

#[test]
fn tab_goes_to_the_nearest_note_above_or_below_when_the_block_has_none() {
    assert_eq!(tab_lands_on("tab-above", &["Plan", "three"], 1), "Plan");
    assert_eq!(tab_lands_on("tab-below", &["Plan", "three"], 2), "three");
}

#[test]
fn tab_between_two_equally_near_notes_goes_to_the_one_below() {
    assert_eq!(tab_lands_on("tab-tie", &["one", "three"], 2), "three");
}
