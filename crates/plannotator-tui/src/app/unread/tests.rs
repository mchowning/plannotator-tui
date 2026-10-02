#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use std::time::{Duration, Instant};

use plannotator_tui_schema::{Kind, is_unread};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent};

use crate::app::review_test_support::{file_app, press};
use crate::app::{App, Focus, Mode};
use crate::doc::Document;
use crate::store::{Location, Store};

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn unread(app: &App, id: &str) -> bool {
    app.open.store.placed().iter().find(|p| p.annotation.id == id).is_some_and(|p| is_unread(p.annotation))
}

/// The pi runner's writer on the same record.
fn runner(app: &App) -> Store {
    let plannotator_tui_schema::Provenance::File { path } = &app.open.source.provenance else {
        unreachable!("a file review")
    };
    let doc = Document::parse(std::fs::read_to_string(path).expect("doc"));
    Store::load(&Location::for_file(&app.data_dir, &app.project, path), &doc).expect("load")
}

/// An attached review with a comment on "one" and an answered, unread thread on "two";
/// the rail is focused on the comment.
fn unread_app(tag: &str) -> (std::path::PathBuf, App, String, String) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
    let comment = app.open.store.placed()[0].annotation.id.clone();
    let thread = app.open.store.placed()[1].annotation.id.clone();
    app.open.store.thread_key(&thread).expect("thread");
    app.open.store.add_agent_reply(&thread, &thread, "Because.".into()).expect("answer");
    app.focus = Focus::Rail;
    app.rail_cursor = 0;
    (root, app, comment, thread)
}

#[test]
fn u_marks_a_comment_unread_and_only_u_marks_it_read() {
    let (_root, mut app, comment, _) = unread_app("unread-comment");
    assert!(!unread(&app, &comment));
    press(&mut app, 'u');
    assert!(unread(&app, &comment));
    press(&mut app, 'e');
    key(&mut app, KeyCode::Enter);
    assert!(unread(&app, &comment), "editing leaves the reminder");
    app.focus = Focus::Rail;
    press(&mut app, 'u');
    assert!(!unread(&app, &comment));
}

#[test]
fn opening_a_thread_reads_it_and_moving_onto_it_does_not() {
    let (_root, mut app, _, thread) = unread_app("unread-open");
    press(&mut app, 'j');
    assert!(unread(&app, &thread), "the cursor passing over is not reading");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Thread(thread.clone()));
    assert!(!unread(&app, &thread));
    key(&mut app, KeyCode::Esc);
    press(&mut app, 'u');
    assert!(unread(&app, &thread), "u marks it unread again");
}

#[test]
fn an_answer_that_lands_while_its_thread_is_open_stays_read() {
    let (_root, mut app, _, thread) = unread_app("unread-panel");
    app.rail_cursor = 1;
    key(&mut app, KeyCode::Enter);
    let through = app.open.store.add_user_reply(&thread, "And three?".into()).expect("reply");
    let start = Instant::now();
    app.poll_record(start);

    let mut cli = runner(&app);
    cli.add_agent_reply(&thread, &through, "Three is next.".into()).expect("answer");
    assert!(app.poll_record(start + Duration::from_millis(250)));
    assert!(!unread(&app, &thread), "you were looking at it");
}

#[test]
fn u_on_a_thread_that_was_already_sent_is_refused() {
    let (_root, mut app, _, thread) = unread_app("unread-historical");
    let ids = vec![thread.clone()];
    app.open.store.record_delivery("agent", &ids).expect("send");
    app.open.store.archive_sent().expect("finish");
    app.open.store.restore_archived(&app.open.doc, &ids).expect("restore");
    app.rail_cursor = app.rail().iter().position(|e| e.annotation.id == thread).expect("on the rail");
    press(&mut app, 'u');
    assert!(!unread(&app, &thread));
    assert_eq!(app.status.as_deref(), Some("read-only: this thread was already sent"));
}
