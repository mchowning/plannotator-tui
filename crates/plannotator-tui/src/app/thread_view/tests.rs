#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use plannotator_tui_schema::{Kind, Provenance, Turn};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;

use crate::app::review_test_support::{draw, file_app, press, reopen};
use crate::app::{App, Focus, Mode};
use crate::store::Location;
use crate::theme::palette;

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn type_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, c));
}

fn record(app: &App) -> Value {
    let Provenance::File { path } = &app.open.source.provenance else { panic!("file") };
    let record = Location::for_file(&app.data_dir, &app.project, path).record;
    serde_json::from_str(&std::fs::read_to_string(record).expect("record")).expect("json")
}

/// An attached review with a thread on "two", the rail focused on it.
fn thread_app(tag: &str) -> (std::path::PathBuf, App, String) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open.store.thread_key(&id).expect("thread");
    app.focus = Focus::Rail;
    (root, app, id)
}

fn render(app: &mut App) -> (Buffer, Option<(u16, u16)>) {
    let mut terminal = Terminal::new(TestBackend::new(160, 45)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let cursor = terminal.get_cursor_position().ok().map(|p| (p.x, p.y));
    (terminal.backend().buffer().clone(), cursor)
}

#[test]
fn enter_on_a_thread_opens_a_panel_about_two_fifths_wide_with_the_passage_highlighted_and_input_focused() {
    let (_root, mut app, id) = thread_app("panel-open");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Thread(id));

    let (buffer, cursor) = render(&mut app);
    let panel = app.geometry.panel.expect("panel drawn");
    let share = f64::from(panel.width) / 160.0;
    assert!((0.40..=0.45).contains(&share), "panel is {} of 160 columns", panel.width);

    let doc = app.geometry.doc;
    let highlighted: Vec<String> = (doc.y..doc.bottom())
        .flat_map(|y| (doc.x..doc.right()).map(move |x| (x, y)))
        .filter_map(|pos| buffer.cell(pos))
        .filter(|c| {
            // The palette's selection style: a background on light themes, reverse video on dark.
            let selection = palette().selection;
            selection.bg.is_none_or(|bg| c.bg == bg) && c.modifier.contains(selection.add_modifier)
        })
        .map(|c| c.symbol().to_owned())
        .collect();
    assert_eq!(highlighted.concat(), "two", "only the passage is highlighted");

    let text = draw(&mut app, 160, 45);
    assert!(text.contains("Why two?"), "{text}");
    let (x, y) = cursor.expect("a text cursor");
    assert!(x >= panel.x && x < panel.right() && y >= panel.y && y < panel.bottom(), "cursor in the panel");
}

#[test]
fn enter_in_the_panel_saves_a_user_reply_before_showing_it() {
    let (_root, mut app, id) = thread_app("panel-reply");
    app.open.store.set_turn(&id, Turn::Running).expect("running");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "And Monday?");
    key(&mut app, KeyCode::Enter);

    let replies = record(&app)["annotations"][0]["replies"].clone();
    assert_eq!(replies[0]["author"], "user");
    assert_eq!(replies[0]["body"], "And Monday?");
    assert!(matches!(app.mode, Mode::Thread(_)), "the panel stays open");
    let text = draw(&mut app, 160, 45);
    assert!(text.contains("And Monday?") && text.contains("working…"), "{text}");
    assert!(app.compose.value().is_empty(), "the input is cleared");
}

#[test]
fn page_keys_scroll_the_panel_and_leave_the_document_alone() {
    let (_root, mut app, id) = thread_app("panel-scroll");
    let long = (1..=40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    app.open.store.add_agent_reply(&id, &id, long).expect("reply");
    key(&mut app, KeyCode::Enter);
    let before = draw(&mut app, 160, 45);
    assert!(before.contains("line 40") && !before.contains("line 1\n"), "opens at the latest text");
    let doc_scroll = app.scroll;

    key(&mut app, KeyCode::PageUp);
    let after = draw(&mut app, 160, 45);
    assert!(!after.contains("line 40"), "the panel scrolled up\n{after}");
    assert_eq!(app.scroll, doc_scroll, "the document did not move");

    key(&mut app, KeyCode::PageDown);
    assert!(draw(&mut app, 160, 45).contains("line 40"));
}

#[test]
fn esc_closes_the_panel() {
    let (_root, mut app, _) = thread_app("panel-esc");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "half typed");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.focus, Focus::Rail);
    draw(&mut app, 160, 45);
    assert!(app.geometry.panel.is_none());
}

#[test]
fn enter_on_a_regular_comment_still_edits_it() {
    let (_root, mut app, _) = file_app("panel-regular");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Edit(_)));
}

#[test]
fn ctrl_t_in_the_panel_retries_a_failed_turn() {
    let (_root, mut app, id) = thread_app("panel-retry");
    app.open.store.set_turn(&id, Turn::Interrupted).expect("interrupted");
    key(&mut app, KeyCode::Enter);
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL))).expect("ctrl-t");
    assert_eq!(app.status.as_deref(), Some("retrying"));
    assert_eq!(record(&app)["annotations"][0]["plannotator_tui_thread"]["turn"]["status"], "idle");
}

#[test]
fn a_historical_thread_opens_read_only() {
    let (_root, mut app, _) = thread_app("panel-historical");
    let Provenance::File { path } = app.open.source.provenance.clone() else { panic!("file") };
    let record_path = Location::for_file(&app.data_dir, &app.project, &path).record;
    let mut data = record(&app);
    data["annotations"][0]["plannotator_tui_thread"]["state"] = "historical".into();
    std::fs::write(&record_path, data.to_string()).expect("write");
    reopen(&mut app);
    app.focus = Focus::Rail;

    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Thread(_)));
    type_text(&mut app, "hello");
    key(&mut app, KeyCode::Enter);
    assert!(
        record(&app)["annotations"][0].get("replies").is_none_or(|r| r.as_array().is_some_and(Vec::is_empty))
    );
    assert!(draw(&mut app, 160, 45).contains("read-only"));
}

#[test]
fn a_detached_thread_opens_and_takes_replies() {
    let (_root, mut app, _) = thread_app("panel-detached");
    let Provenance::File { path } = app.open.source.provenance.clone() else { panic!("file") };
    std::fs::write(&path, "# Plan\n\none\n\nfour\n").expect("edit");
    press(&mut app, 'r');
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Thread(_)));
    assert!(draw(&mut app, 160, 45).contains("passage since changed"));
    type_text(&mut app, "Still relevant?");
    key(&mut app, KeyCode::Enter);
    assert_eq!(record(&app)["annotations"][0]["replies"][0]["body"], "Still relevant?");
}

#[test]
fn every_thread_action_is_on_disk_at_once_and_a_reopen_restores_it_all() {
    let (_root, mut app, id) = thread_app("persist");
    let after_start = record(&app);
    assert_eq!(after_start["annotations"][0]["plannotator_tui_thread"]["state"], "live");

    let fork = plannotator_tui_schema::Fork {
        session_path: "/tmp/fork.jsonl".into(),
        origin_session: "main".into(),
        fork_point: "main.jsonl#e1".into(),
        model: "claude-bridge/claude-haiku-4-5".into(),
        thinking: "low".into(),
    };
    app.open.store.set_fork(&id, fork).expect("fork");
    app.open.store.add_agent_reply(&id, &id, "Two follows one.".into()).expect("reply");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "And three?");
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        record(&app)["annotations"][0]["replies"][1]["body"],
        "And three?",
        "saved before the next key"
    );
    app.open
        .store
        .set_turn(&id, Turn::Failed { detail: "rate limited (429)".into(), retryable: true })
        .expect("fail");

    let before = record(&app);
    let threads_before = format!("{:?}", app.open.store.threads().expect("threads"));
    reopen(&mut app);
    assert_eq!(record(&app), before);
    assert_eq!(format!("{:?}", app.open.store.threads().expect("threads")), threads_before);
    let thread = &before["annotations"][0]["plannotator_tui_thread"];
    assert_eq!(thread["fork"]["session_path"], "/tmp/fork.jsonl");
    assert_eq!(thread["turn"]["status"], "failed");
}

#[test]
fn the_panel_title_fits_and_the_footer_names_the_panel_keys() {
    let (_root, mut app, _) = thread_app("panel-help");
    key(&mut app, KeyCode::Enter);
    let screen = draw(&mut app, 160, 45);
    assert!(screen.contains(" thread · esc closes "), "{screen}");
    let footer = screen.lines().last().expect("footer");
    assert!(footer.contains("enter reply · ctrl-n note · ctrl-t retry · pgup/pgdn scroll"), "{footer}");
}

#[test]
fn ctrl_n_in_the_panel_saves_a_note_for_the_main_agent_that_the_fork_is_not_asked() {
    let (_root, mut app, id) = thread_app("panel-note");
    app.open.store.add_agent_reply(&id, &id, "Two follows one.".into()).expect("answer");
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "Rename it later");
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))).expect("ctrl-n");

    let replies = record(&app)["annotations"][0]["replies"].clone();
    assert_eq!(replies[1]["body"], "Rename it later");
    assert_eq!(
        (replies[1]["author"].as_str(), replies[1]["plannotator_tui_for"].as_str()),
        (Some("user"), Some("main"))
    );
    let thread = app.open.store.threads().expect("threads").remove(0);
    assert!(!thread.thread.needs_turn(thread.annotation), "the fork is not asked");
    assert!(app.compose.value().is_empty(), "the input is cleared");
    let screen = draw(&mut app, 160, 45);
    let author_row = |l: &str| l.split('│').any(|cell| cell.trim() == "note");
    assert!(screen.lines().any(author_row), "the note is headed `note`\n{screen}");
    key(&mut app, KeyCode::Esc);
    let screen = draw(&mut app, 160, 45);
    assert!(screen.contains("note: Rename it later"), "the rail preview says `note:`\n{screen}");
}

#[test]
fn a_note_needs_no_attached_agent_session() {
    let (_root, mut app, _) = thread_app("panel-note-unattached");
    app.agent_session = None;
    key(&mut app, KeyCode::Enter);
    type_text(&mut app, "For later");
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))).expect("ctrl-n");
    assert_eq!(record(&app)["annotations"][0]["replies"][0]["body"], "For later");
}
