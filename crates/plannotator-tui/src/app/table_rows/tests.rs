#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use plannotator_tui_schema::{DocumentSource, Kind, Provenance};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus, Mode};
use crate::delivery::Discard;
use crate::theme::palette;

const DOC: &str = "# T\n\n| Name | Age |\n|---|---|\n| Ann | 30 |\n| Bob | 41 |\n\nafter\n";

/// A transient document: nothing is written to the Plannotator data directory.
fn table_app() -> App {
    let source = DocumentSource::new(DOC.to_owned(), "t.md", true, Provenance::Stdin);
    let mut app = App::open(source, 60, Box::new(Discard)).expect("app opens");
    app.data_dir = std::env::temp_dir().join(format!("plannotator-tui-table-rows-{}", std::process::id()));
    app
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

/// One frame: the text of the document rows drawn with the selected-block background,
/// and the footer.
fn highlighted_and_footer(app: &mut App) -> (Vec<String>, String) {
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let doc = app.geometry.doc;
    let row_text = |y| {
        (doc.x..doc.right())
            .filter_map(|x| buffer.cell((x, y)))
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>()
    };
    let highlighted = (doc.y..doc.bottom())
        .filter(|&y| buffer.cell((doc.x, y)).is_some_and(|c| c.bg == palette().block_bg))
        .map(|y| row_text(y).trim().to_owned())
        .collect();
    let footer = (0..buffer.area.width)
        .filter_map(|x| buffer.cell((x, buffer.area.height - 1)))
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    (highlighted, footer)
}

#[test]
fn j_and_k_step_through_a_tables_rows_one_at_a_time() {
    let mut app = table_app();
    highlighted_and_footer(&mut app);
    key(&mut app, KeyCode::Char('j'));
    let (highlighted, footer) = highlighted_and_footer(&mut app);
    assert_eq!(highlighted, ["│ Name │ Age │"], "entering a table selects its header row");
    assert!(footer.contains("block 2/3 · row 1/3"), "{footer}");

    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(highlighted_and_footer(&mut app).0, ["│ Bob  │ 41  │"]);

    key(&mut app, KeyCode::Char('j'));
    assert_eq!(highlighted_and_footer(&mut app).0, ["after"], "past the last row is the next block");
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(
        highlighted_and_footer(&mut app).0,
        ["│ Bob  │ 41  │"],
        "coming back up lands on the last row"
    );
}

#[test]
fn c_on_a_table_row_comments_on_that_row_alone() {
    let mut app = table_app();
    highlighted_and_footer(&mut app);
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('c'));
    assert!(matches!(app.mode, Mode::Compose));
    let pending = app.pending.as_ref().expect("a pending selection");
    assert_eq!(app.open.doc.source.get(pending.range.clone()), Some("| Ann | 30 |"));
}

#[test]
fn a_note_picked_in_the_rail_selects_its_table_row() {
    let mut app = table_app();
    app.add_quote_annotation("Bob", Kind::Comment, "who?".into()).expect("comment");
    highlighted_and_footer(&mut app);
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Esc);
    assert_eq!(highlighted_and_footer(&mut app).0, ["│ Bob  │ 41  │"]);
}

#[test]
fn paging_into_a_tall_table_selects_a_row_on_screen() {
    let body = (1..=40).map(|n| format!("| r{n} | x |")).collect::<Vec<_>>().join("\n");
    let source =
        DocumentSource::new(format!("# T\n\n| a | b |\n|---|---|\n{body}"), "t.md", true, Provenance::Stdin);
    let mut app = App::open(source, 60, Box::new(Discard)).expect("app opens");
    highlighted_and_footer(&mut app);
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))).expect("ctrl-d");
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))).expect("ctrl-d");
    let highlighted = highlighted_and_footer(&mut app).0;
    assert_eq!(highlighted.len(), 1, "one row, and it is on screen: {highlighted:?}");
}

#[test]
fn x_on_a_table_row_removes_only_that_rows_notes() {
    let mut app = table_app();
    app.add_quote_annotation("Ann", Kind::Comment, "a".into()).expect("comment");
    app.add_quote_annotation("Bob", Kind::Comment, "b".into()).expect("comment");
    highlighted_and_footer(&mut app);
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(
        app.status.as_deref(),
        Some("no notes on this row"),
        "the header has none, though the table does"
    );

    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('x'));
    let footer = highlighted_and_footer(&mut app).1;
    assert!(footer.contains("remove the note on this row? y remove"), "{footer}");
    key(&mut app, KeyCode::Char('y'));
    let left: Vec<_> = app.open.store.placed().iter().map(|p| p.annotation.body.clone()).collect();
    assert_eq!(left, ["b"]);
    assert_eq!(app.status.as_deref(), Some("removed 1 annotation(s) on row"));
}
