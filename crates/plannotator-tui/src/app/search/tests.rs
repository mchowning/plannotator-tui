#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use plannotator_tui_schema::{Kind, Provenance};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent};

use crate::app::review_test_support::{draw, file_app, press, reopen};
use crate::app::{App, Mode};
use crate::theme::palette;

const DOC: &str = "# Plan\n\nThe **login page** ships first.\n\nA paragraph about nothing.\n\nThen the login flow, and the login tests.\n";

fn app_with(tag: &str, content: &str) -> (std::path::PathBuf, App) {
    let (root, mut app, _) = file_app(tag);
    let Provenance::File { path } = app.open.source.provenance.clone() else { panic!("file") };
    std::fs::write(&path, content).expect("document");
    reopen(&mut app);
    draw(&mut app, 120, 30);
    (root, app)
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn search(app: &mut App, query: &str) {
    press(app, '/');
    query.chars().for_each(|c| press(app, c));
    key(app, KeyCode::Enter);
}

/// The selected block and the rendered text starting at the cursor.
fn at_cursor(app: &App, len: usize) -> (usize, String) {
    let row = app.open.layout.row(app.cursor.0).expect("cursor on a text row");
    (app.selected, row.line.to_string().chars().skip(app.cursor.1).take(len).collect())
}

#[test]
fn slash_jumps_to_the_next_match_and_n_and_shift_n_step_through_them_wrapping_around() {
    let (_root, mut app) = app_with("search-steps", DOC);
    search(&mut app, "login");
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()));
    press(&mut app, 'n');
    assert_eq!(at_cursor(&app, 10), (3, "login flow".into()));
    press(&mut app, 'n');
    assert_eq!(at_cursor(&app, 11), (3, "login tests".into()));
    press(&mut app, 'n');
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()));
    assert!(app.status.as_deref().is_some_and(|s| s.contains("wrapped")), "{:?}", app.status);
    press(&mut app, 'N');
    assert_eq!(at_cursor(&app, 11), (3, "login tests".into()));
    // An empty query searches for the last one again.
    search(&mut app, "");
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()));
}

#[test]
fn search_matches_the_rendered_text_not_the_markup() {
    let (_root, mut app) = app_with("search-rendered", DOC);
    search(&mut app, "login page");
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()));
}

#[test]
fn notes_are_not_searched() {
    let (_root, mut app) = app_with("search-notes", DOC);
    app.add_block_annotation(2, Kind::Comment, "zebra crossing".into()).expect("note");
    search(&mut app, "zebra");
    assert_eq!(app.selected, 0, "the cursor stays put");
    assert!(app.status.as_deref().is_some_and(|s| s.contains("not found")), "{:?}", app.status);
}

#[test]
fn a_match_split_across_wrapped_rows_is_found() {
    let paragraph = format!("{}needle haystack\n", "word ".repeat(7));
    let (_root, mut app) = app_with("search-wrap", &paragraph);
    draw(&mut app, 50, 20);
    let rows: Vec<String> = (0..app.open.layout.total_rows)
        .filter_map(|r| app.open.layout.row(r).map(|row| row.line.to_string()))
        .collect();
    assert!(rows.len() > 1 && !rows.iter().any(|r| r.contains("needle haystack")), "{rows:?}");
    search(&mut app, "needle haystack");
    assert_eq!(at_cursor(&app, 6), (0, "needle".into()));
}

#[test]
fn typing_jumps_to_the_first_match_from_where_slash_was_pressed_as_each_key_arrives() {
    let (_root, mut app) = app_with("search-live", DOC);
    press(&mut app, '/');
    "login".chars().for_each(|c| press(&mut app, c));
    let screen = draw(&mut app, 120, 30);
    assert!(screen.lines().last().is_some_and(|l| l.starts_with("/login")), "{screen}");
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()), "moved before enter");
    assert!(highlighted(&mut app, "login flow"), "matches are highlighted while typing");
    " f".chars().for_each(|c| press(&mut app, c));
    assert_eq!(at_cursor(&app, 10), (3, "login flow".into()));
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Backspace);
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()), "searched again from the start");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(at_cursor(&app, 10), (1, "login page".into()), "enter keeps the match");
}

#[test]
fn esc_while_typing_puts_the_cursor_and_view_back() {
    let (_root, mut app) = app_with("search-cancel", DOC);
    app.select_block(2);
    let before = (app.selected, app.cursor, app.scroll);
    press(&mut app, '/');
    "login".chars().for_each(|c| press(&mut app, c));
    assert_ne!(app.selected, before.0, "the live search moved");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!((app.selected, app.cursor, app.scroll), before);
    assert!(!highlighted(&mut app, "login flow"), "a cancelled search leaves nothing highlighted");
}

/// Whether the cell where `text` starts on screen wears the search style.
fn highlighted(app: &mut App, text: &str) -> bool {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    (0..30).any(|y| {
        let row: String =
            (0..120).filter_map(|x| buffer.cell((x, y))).map(ratatui::buffer::Cell::symbol).collect();
        row.find(text).is_some_and(|start| {
            let x = row[..start].chars().count() as u16;
            buffer.cell((x, y)).is_some_and(|c| c.bg == palette().search_bg)
        })
    })
}

#[test]
fn matches_are_highlighted_until_esc_clears_them_and_only_then_esc_quits() {
    let (_root, mut app) = app_with("search-highlight", DOC);
    search(&mut app, "login");
    assert!(highlighted(&mut app, "login flow"), "every match is highlighted, not only the current one");
    key(&mut app, KeyCode::Esc);
    assert!(!highlighted(&mut app, "login flow"));
    assert!(!app.quit, "the first esc only cleared the highlight");
    key(&mut app, KeyCode::Esc);
    assert!(app.quit);
}
