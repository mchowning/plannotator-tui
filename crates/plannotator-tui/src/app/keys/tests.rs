#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic, reason = "tests assert by panicking")]

use plannotator_tui_schema::{Kind, Provenance, Turn};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::review_test_support::{draw, file_app, folder_app, press, reply_app};
use crate::app::{App, Focus, Mode};
use crate::store::Location;

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code))).expect("key");
}

fn ctrl_r(app: &mut App) {
    app.handle_event(&Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))).expect("ctrl-r");
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_event(&Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }))
        .expect("mouse");
}

fn type_text(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, c));
}

/// The popup's rows inside its border, trimmed, read cell by cell from one frame.
fn popup(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let rect = app.geometry.keys.expect("the key list was drawn");
    let buffer = terminal.backend().buffer();
    (rect.y + 1..rect.bottom() - 1)
        .map(|y| {
            (rect.x + 1..rect.right() - 1)
                .filter_map(|x| buffer.cell((x, y)))
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .collect()
}

fn footer(app: &mut App, width: u16) -> String {
    draw(app, width, 24).lines().last().expect("footer").trim_end().to_owned()
}

fn record_bytes(app: &App) -> Option<Vec<u8>> {
    let Provenance::File { path } = &app.open.source.provenance else { panic!("a file review") };
    std::fs::read(Location::for_file(&app.data_dir, &app.project, path).record).ok()
}

/// A row whose key column is exactly `keys`.
fn has_row(rows: &[String], keys: &str) -> bool {
    rows.iter().any(|row| row.split("  ").next() == Some(keys))
}

/// The rows under `heading`, up to the next blank line.
fn group<'a>(rows: &'a [String], heading: &str) -> &'a [String] {
    let start = rows.iter().position(|r| r == heading).map_or(rows.len(), |i| i + 1);
    let rest = rows.get(start..).unwrap_or_default();
    let end = rest.iter().position(String::is_empty).unwrap_or(rest.len());
    &rest[..end]
}

// ----- the popup (R16, R17, R18) -------------------------------------------------------

#[test]
fn question_mark_opens_the_list_with_the_focused_panes_group_first() {
    let (_root, mut app, _) = file_app("keys-doc");
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::Keys);
    assert_eq!(popup(&mut app, 160, 120).first().map(String::as_str), Some("Document"));

    let (_root, mut app, _) = file_app("keys-rail");
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.focus = Focus::Rail;
    press(&mut app, '?');
    assert_eq!(popup(&mut app, 160, 120).first().map(String::as_str), Some("Notes rail"));

    let (_root, mut app, _) = folder_app("keys-tree");
    app.focus = Focus::Tree;
    press(&mut app, '?');
    assert_eq!(popup(&mut app, 160, 120).first().map(String::as_str), Some("File tree"));
}

#[test]
fn an_unattached_file_review_lists_no_thread_keys_and_no_s() {
    let (_root, mut app, _) = file_app("keys-unattached");
    press(&mut app, '?');
    let rows = popup(&mut app, 160, 120);
    assert!(!rows.iter().any(|r| r.contains("ctrl-r")), "{rows:#?}");
    assert!(!has_row(&rows, "S"), "{rows:#?}");
    assert!(!rows.iter().any(|r| r == "Thread panel"), "{rows:#?}");
    assert!(has_row(&rows, "E") && has_row(&rows, "m"), "{rows:#?}");
}

#[test]
fn an_attached_review_lists_ctrl_r_where_it_is_used() {
    let (_root, mut app, _) = file_app("keys-attached");
    app.attach_agent_session("pi".into());
    press(&mut app, '?');
    let rows = popup(&mut app, 160, 120);
    for heading in ["Comment box (while writing)", "Notes rail", "Thread panel"] {
        assert!(has_row(group(&rows, heading), "ctrl-r"), "no ctrl-r under {heading}: {rows:#?}");
    }
}

#[test]
fn a_reply_review_lists_s_and_e_and_no_review_menu() {
    let (_root, mut app, _) = reply_app("keys-reply");
    press(&mut app, '?');
    let rows = popup(&mut app, 160, 120);
    assert!(has_row(&rows, "S") && has_row(&rows, "E"), "{rows:#?}");
    for menu in ["m", "R", "F", "U", "H"] {
        assert!(!has_row(&rows, menu), "{menu} listed in a reply review: {rows:#?}");
    }
}

#[test]
fn only_a_folder_review_lists_the_tree() {
    let (_root, mut app, _) = file_app("keys-no-tree");
    press(&mut app, '?');
    let rows = popup(&mut app, 160, 120);
    assert!(!rows.iter().any(|r| r == "File tree") && !has_row(&rows, "t"), "{rows:#?}");

    let (_root, mut app, _) = folder_app("keys-tree-t");
    press(&mut app, '?');
    let rows = popup(&mut app, 160, 120);
    assert!(rows.iter().any(|r| r == "File tree") && has_row(&rows, "t"), "{rows:#?}");
}

#[test]
fn every_review_lists_the_selection_toolbar_under_its_own_heading() {
    let shapes: [fn(&str) -> _; 3] = [file_app, reply_app, folder_app];
    for shape in shapes {
        let (_root, mut app, _) = shape("keys-toolbar");
        press(&mut app, '?');
        let rows = popup(&mut app, 160, 120);
        let toolbar = group(&rows, "Selection toolbar (while text is selected)");
        for key in ["a", "c", "d"] {
            assert!(has_row(toolbar, key), "{key} missing: {rows:#?}");
        }
    }
}

#[test]
fn question_mark_and_esc_close_the_list() {
    let (_root, mut app, _) = file_app("keys-close");
    press(&mut app, '?');
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::Browse);
    press(&mut app, '?');
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Browse);
    assert!(!app.quit, "esc closes the list, it does not quit");
}

#[test]
fn a_click_outside_the_list_closes_it_and_one_inside_does_not() {
    let (_root, mut app, _) = file_app("keys-click");
    press(&mut app, '?');
    popup(&mut app, 160, 45);
    let rect = app.geometry.keys.expect("rect");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), rect.x + 2, rect.y + 2);
    assert_eq!(app.mode, Mode::Keys, "a click inside keeps it open");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 0, rect.bottom() + 1);
    assert_eq!(app.mode, Mode::Browse);
}

#[test]
fn other_keys_do_nothing_while_the_list_is_open() {
    let (_root, mut app, delivery) = file_app("keys-swallow");
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    let before = record_bytes(&app);
    press(&mut app, '?');
    for ch in ['q', 'E', 'x'] {
        press(&mut app, ch);
        assert_eq!(app.mode, Mode::Keys, "{ch} closed the list");
    }
    assert!(!app.quit, "q quit from under the list");
    assert!(delivery.calls.borrow().is_empty(), "E sent from under the list");
    assert_eq!(record_bytes(&app), before, "the record changed");
}

#[test]
fn a_list_taller_than_the_screen_scrolls_with_j_k_and_the_wheel() {
    let (_root, mut app, _) = file_app("keys-scroll");
    app.attach_agent_session("pi".into());
    press(&mut app, '?');
    let first = popup(&mut app, 80, 12);
    assert!(!first.iter().any(|r| r.ends_with("close") && r.starts_with("esc")), "{first:#?}");
    for _ in 0..200 {
        press(&mut app, 'j');
    }
    let bottom = popup(&mut app, 80, 12);
    assert_eq!(
        bottom.last().map(|r| r.split_whitespace().collect::<Vec<_>>()),
        Some(vec!["esc", "close"]),
        "the thread panel's last row is in view: {bottom:#?}"
    );
    for _ in 0..200 {
        press(&mut app, 'k');
    }
    assert_eq!(popup(&mut app, 80, 12), first, "k scrolls back to the top");

    mouse(&mut app, MouseEventKind::ScrollDown, 40, 6);
    let wheeled = popup(&mut app, 80, 12);
    assert_ne!(wheeled, first, "the wheel scrolls");
    assert_eq!(app.mode, Mode::Keys);
}

// ----- `?` is text where text is typed (R19) --------------------------------------------

#[test]
fn a_question_mark_is_text_in_the_comment_box_the_edit_box_and_the_thread_panel() {
    let (_root, mut app, _) = file_app("keys-text");
    app.attach_agent_session("pi".into());
    press(&mut app, 'c');
    type_text(&mut app, "why?");
    key(&mut app, KeyCode::Enter);
    let placed = app.open.store.placed();
    assert_eq!(placed[0].annotation.body, "why?");
    let id = placed[0].annotation.id.clone();

    app.focus = Focus::Rail;
    press(&mut app, 'e');
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::Edit(id.clone()));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.open.store.placed()[0].annotation.body, "why??");

    app.open.store.thread_key(&id).expect("thread");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Thread(id.clone()));
    type_text(&mut app, "why?");
    key(&mut app, KeyCode::Enter);
    let bodies: Vec<String> = plannotator_tui_schema::thread::messages(app.open.store.placed()[0].annotation)
        .iter()
        .map(|m| m.body.to_owned())
        .collect();
    assert_eq!(bodies.last().map(String::as_str), Some("why?"), "{bodies:?}");
}

#[test]
fn a_question_mark_does_not_open_the_list_from_prompts_pickers_or_menus() {
    let (_root, mut app, _) = file_app("keys-confirm");
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    press(&mut app, 'q');
    assert_eq!(app.mode, Mode::ConfirmQuit);
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::ConfirmQuit);

    let (_root, mut app, _) = file_app("keys-menu");
    press(&mut app, 'm');
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::ReviewMenu);

    let (_root, mut app, _) = file_app("keys-archive");
    app.mode = Mode::Archive;
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::Archive);

    let (_root, mut app, _) = file_app("keys-pick");
    app.mode = Mode::Pick;
    press(&mut app, '?');
    assert_eq!(app.mode, Mode::Pick);
}

// ----- `? keys` in the footer (R21, R22) ------------------------------------------------

#[test]
fn the_key_help_starts_with_the_pointer_when_it_fits() {
    let (_root, mut app, _) = file_app("pointer-doc");
    assert!(
        footer(&mut app, 160).ends_with(" ? keys · i move · v select · c comment · E send · tab · q quit"),
        "{:?}",
        footer(&mut app, 160)
    );

    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.status = None;
    app.focus = Focus::Rail;
    assert!(
        footer(&mut app, 160).ends_with(" ? keys · j/k · e edit · x remove · tab · q quit"),
        "{:?}",
        footer(&mut app, 160)
    );

    let (_root, mut app, _) = folder_app("pointer-tree");
    app.focus = Focus::Tree;
    assert!(
        footer(&mut app, 160).ends_with(" ? keys · j/k · enter open · . hidden · E send · t hide · q quit"),
        "{:?}",
        footer(&mut app, 160)
    );
}

#[test]
fn the_key_help_is_todays_when_the_pointer_does_not_fit() {
    let (_root, mut app, _) = reply_app("pointer-narrow");
    app.roam = true;
    let line = footer(&mut app, 80);
    assert!(
        line.ends_with("hjkl move · v select · c comment · esc blocks · S send+quit · q quit"),
        "{line:?}"
    );
    assert!(!line.contains("? keys"), "{line:?}");
}

#[test]
fn a_status_still_takes_the_key_helps_columns_at_80() {
    let (_root, mut app, _) = file_app("pointer-status");
    let status = "restored 3 notes from the archive for this file";
    app.set_status(status.to_owned());
    let line = footer(&mut app, 80);
    assert!(line.contains(status), "{line:?}");
}

// ----- thread hints (R23, R24) ----------------------------------------------------------

/// An attached review with a comment on "one", a looks-good mark on "two", and a thread on
/// "three"; the rail is focused.
fn hint_app(tag: &str) -> (std::path::PathBuf, App, [String; 3]) {
    let (root, mut app, _) = file_app(tag);
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.add_quote_annotation("two", Kind::LooksGood, String::new()).expect("looks good");
    app.add_quote_annotation("three", Kind::Comment, "B".into()).expect("comment");
    let ids: Vec<String> = app.rail().iter().map(|e| e.annotation.id.clone()).collect();
    app.open.store.thread_key(&ids[2]).expect("thread");
    app.focus = Focus::Rail;
    app.status = None;
    (root, app, [ids[0].clone(), ids[1].clone(), ids[2].clone()])
}

#[test]
fn the_rail_names_the_thread_key_that_applies_to_the_selected_note() {
    let (_root, mut app, [_, _, thread]) = hint_app("hints-rail");
    app.rail_cursor = 0;
    assert!(footer(&mut app, 160).contains("ctrl-r thread"), "{:?}", footer(&mut app, 160));

    app.rail_cursor = 1;
    assert!(!footer(&mut app, 160).contains("ctrl-r"), "{:?}", footer(&mut app, 160));

    app.rail_cursor = 2;
    let line = footer(&mut app, 160);
    assert!(line.contains("enter open") && !line.contains("ctrl-r"), "{line:?}");

    for broken in [Turn::Failed { detail: "rate limited (429)".into(), retryable: true }, Turn::Interrupted] {
        app.open.store.set_turn(&thread, broken).expect("break");
        let line = footer(&mut app, 160);
        assert!(line.contains("enter open · ctrl-r retry"), "{line:?}");
    }
}

#[test]
fn the_new_comment_box_names_ctrl_r_when_attached_and_the_edit_box_does_not() {
    let (_root, mut app, _) = file_app("hints-compose");
    app.attach_agent_session("pi".into());
    press(&mut app, 'c');
    let screen = draw(&mut app, 160, 45);
    let border = screen.lines().find(|l| l.contains("╭ comment")).expect("compose box");
    assert!(border.contains("╭ comment · enter saves · ctrl-r thread"), "{border:?}");
    key(&mut app, KeyCode::Esc);

    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.focus = Focus::Rail;
    press(&mut app, 'e');
    let screen = draw(&mut app, 160, 45);
    let border = screen.lines().find(|l| l.contains("╭ edit")).expect("edit box");
    assert!(border.contains("╭ edit · enter saves · alt+enter new line"), "{border:?}");
}

#[test]
fn an_unattached_review_names_no_thread_keys() {
    let (_root, mut app, _) = file_app("hints-unattached");
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    app.focus = Focus::Rail;
    app.status = None;
    assert!(!footer(&mut app, 160).contains("ctrl-r"), "{:?}", footer(&mut app, 160));

    app.focus = Focus::Document;
    press(&mut app, 'c');
    let screen = draw(&mut app, 160, 45);
    let border = screen.lines().find(|l| l.contains("╭ comment")).expect("compose box");
    assert!(border.contains("╭ comment · enter saves · alt+enter new line"), "{border:?}");
}

#[test]
fn the_thread_panels_key_help_is_unchanged() {
    let (_root, mut app, _) = file_app("hints-panel");
    app.attach_agent_session("pi".into());
    app.add_quote_annotation("one", Kind::Comment, "A".into()).expect("comment");
    let id = app.open.store.placed()[0].annotation.id.clone();
    app.open.store.thread_key(&id).expect("thread");
    app.focus = Focus::Rail;
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Thread(id));
    ctrl_r(&mut app);
    app.status = None;
    assert!(
        footer(&mut app, 160).ends_with(" enter reply · ctrl-r retry · pgup/pgdn scroll · esc close"),
        "{:?}",
        footer(&mut app, 160)
    );
}
