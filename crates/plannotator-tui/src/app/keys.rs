//! The key list: `?` opens a popup of the keys that act in this review, grouped by where
//! they are used, the focused pane's group first. Also the key help's pointer to it and
//! the rail's thread hint. Drawn and dismissed like the review menu. `/` filters it.

use plannotator_tui_schema::{Kind, Turn};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr as _;

use super::{App, Focus, Mode};

mod filter;
#[cfg(test)]
mod tests;

pub(super) use filter::KeyFilter;

/// Where a key is used; one heading each in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Document,
    Rail,
    Tree,
    Anywhere,
    Moving,
    Toolbar,
    CommentBox,
    ThreadPanel,
}

impl Group {
    fn heading(self) -> &'static str {
        match self {
            Self::Document => "Document",
            Self::Rail => "Notes rail",
            Self::Tree => "File tree",
            Self::Anywhere => "Anywhere",
            Self::Moving => "Moving and selecting (after i or v)",
            Self::Toolbar => "Selection toolbar (while text is selected)",
            Self::CommentBox => "Comment box (while writing)",
            Self::ThreadPanel => "Thread panel",
        }
    }
}

/// Which reviews a key acts in. Judged by what the review is, not by the moment.
#[derive(Debug, Clone, Copy)]
enum When {
    Always,
    /// An agent session is attached, so thread keys do something.
    Attached,
    /// A review of an agent's reply.
    Reply,
    /// A file or folder review.
    FileReview,
    /// A folder review, which has a tree.
    Tree,
    /// A reply review with more than one message to pick from.
    Picker,
}

/// Groups in display order after the focused one.
const GROUPS: [Group; 8] = [
    Group::Anywhere,
    Group::Document,
    Group::Rail,
    Group::Tree,
    Group::Moving,
    Group::Toolbar,
    Group::CommentBox,
    Group::ThreadPanel,
];

/// Every key: (group, keys, what it does, when it applies).
const KEY_LIST: &[(Group, &str, &str, When)] = &[
    (Group::Anywhere, "tab", "switch pane", When::Always),
    (Group::Anywhere, "?", "this list", When::Always),
    (Group::Anywhere, "E", "send new notes", When::Always),
    (Group::Anywhere, "S", "send every note and quit", When::Reply),
    (Group::Anywhere, "m", "review menu", When::FileReview),
    (Group::Anywhere, "R", "resend every sent note", When::FileReview),
    (Group::Anywhere, "F", "finish review: archive sent notes", When::FileReview),
    (Group::Anywhere, "U", "undo finish", When::FileReview),
    (Group::Anywhere, "H", "archived notes", When::FileReview),
    (Group::Anywhere, "r", "reload the file", When::FileReview),
    (Group::Anywhere, "p", "pick another message", When::Picker),
    (Group::Anywhere, "t", "show or hide the tree", When::Tree),
    (Group::Anywhere, "q", "quit", When::Always),
    (Group::Document, "j/k", "next / previous block, table row or list item", When::Always),
    (Group::Document, "g/G", "first / last block", When::Always),
    (Group::Document, "ctrl-d/ctrl-u", "half a page down / up", When::Always),
    (Group::Document, "/", "search the document text", When::Always),
    (Group::Document, "n/N", "next / previous match", When::Always),
    (Group::Document, "c/enter", "comment on the block, table row or list item", When::Always),
    (Group::Document, "x", "remove the notes on the block, table row or list item", When::Always),
    (Group::Document, "i", "move a cursor within the text", When::Always),
    (Group::Document, "v", "start selecting text", When::Always),
    (Group::Document, "V", "start selecting whole rows", When::Always),
    (Group::Document, "→/l", "go to the notes rail", When::Always),
    (Group::Document, "esc", "clear the selection, or quit", When::Always),
    (Group::Rail, "j/k", "next / previous note", When::Always),
    (Group::Rail, "home/end", "first / last note", When::Always),
    (Group::Rail, "enter", "edit the note, or open its thread", When::Always),
    (Group::Rail, "e", "edit the note", When::Always),
    (Group::Rail, "x/del", "remove the note", When::Always),
    (Group::Rail, "u", "mark the note unread, or read", When::Always),
    (Group::Rail, "n/N", "next / previous unread note", When::Always),
    (Group::Rail, "ctrl-t", "make a comment a thread, or retry a failed turn", When::Attached),
    (Group::Rail, "ctrl-r", "resolve or unresolve a thread", When::Attached),
    (Group::Rail, "esc/←/h", "back to the document", When::Always),
    (Group::Tree, "j/k", "move", When::Tree),
    (Group::Tree, "enter/l", "open a file, or expand a folder", When::Tree),
    (Group::Tree, ".", "show or hide dot-prefixed entries", When::Tree),
    (Group::Tree, "esc", "back to the document", When::Tree),
    (Group::Moving, "hjkl", "move the cursor", When::Always),
    (Group::Moving, "w/b", "next / previous word", When::Always),
    (Group::Moving, "e", "end of the word", When::Always),
    (Group::Moving, "0/$", "start / end of the row", When::Always),
    (Group::Moving, "enter", "finish the selection", When::Always),
    (Group::Moving, "v/V", "switch to text / whole rows; again to finish", When::Always),
    (Group::Moving, "esc", "back to blocks", When::Always),
    (Group::Toolbar, "a", "looks good", When::Always),
    (Group::Toolbar, "c", "comment", When::Always),
    (Group::Toolbar, "d", "delete this", When::Always),
    (Group::Toolbar, "esc", "clear the selection", When::Always),
    (Group::CommentBox, "enter", "save", When::Always),
    (Group::CommentBox, "alt+enter", "new line", When::Always),
    (Group::CommentBox, "ctrl-t", "save as a thread", When::Attached),
    (Group::CommentBox, "esc", "cancel", When::Always),
    (Group::ThreadPanel, "enter", "send the reply", When::Attached),
    (
        Group::ThreadPanel,
        "ctrl-n",
        "save as a note for the main agent, not answered in the thread",
        When::Attached,
    ),
    (Group::ThreadPanel, "ctrl-t", "retry a failed turn", When::Attached),
    (Group::ThreadPanel, "ctrl-r", "resolve or unresolve", When::Attached),
    (Group::ThreadPanel, "pgup/pgdn", "scroll", When::Attached),
    (Group::ThreadPanel, "esc", "close", When::Attached),
];

/// Width of the key column, gap included.
const KEY_COLUMN: usize = 16;
const KEYS_HELP: &str = " j/k scroll \u{b7} / filter \u{b7} ? or esc close ";
const NO_MATCH: &str = "no keys match";

/// The key help's pointer to this list.
const POINTER: &str = "? keys \u{b7} ";

/// `help` with the pointer in front when the whole of it fits in `available` columns.
pub(super) fn with_pointer(help: String, available: u16) -> String {
    let pointed = format!("{POINTER}{help}");
    if pointed.width() <= usize::from(available) { pointed } else { help }
}

impl App {
    fn applies(&self, when: When) -> bool {
        match when {
            When::Always => true,
            When::Attached => self.attached_session().is_some(),
            When::Reply => !self.is_file_review(),
            When::FileReview => self.is_file_review(),
            When::Tree => self.tree.is_some(),
            When::Picker => !self.is_file_review() && self.candidates.len() > 1,
        }
    }

    /// The list's lines: each group that has a key here and in `filter`, the focused
    /// pane's first.
    fn key_list_lines(&self, filter: &KeyFilter) -> Vec<Line<'static>> {
        let focused = match self.focus {
            Focus::Document => Group::Document,
            Focus::Rail => Group::Rail,
            Focus::Tree => Group::Tree,
        };
        let order = std::iter::once(focused).chain(GROUPS.into_iter().filter(|&g| g != focused));
        let groups: Vec<Vec<Line<'static>>> = order
            .map(|group| {
                let rows = KEY_LIST
                    .iter()
                    .filter(|(g, keys, does, when)| {
                        *g == group && self.applies(*when) && filter.keeps(keys, does)
                    })
                    .map(|(_, keys, does, _)| Line::from(format!("{keys:<KEY_COLUMN$}{does}")));
                std::iter::once(Line::from(Span::raw(group.heading()).bold().fg(Color::Cyan)))
                    .chain(rows)
                    .collect::<Vec<_>>()
            })
            // A heading alone means no key in that group acts here.
            .filter(|lines| lines.len() > 1)
            .collect();
        groups.join(&Line::default())
    }

    pub(super) fn open_key_list(&mut self) {
        self.keys_scroll = 0;
        self.keys_filter = KeyFilter::default();
        self.mode = Mode::Keys;
    }

    fn scroll_key_list(&mut self, delta: i64) {
        let max = self.geometry.keys_max_scroll;
        self.keys_scroll = (self.keys_scroll as i64 + delta).clamp(0, max as i64) as usize;
    }

    /// While typing a filter, keys are its text; `Enter` keeps it and `Esc` drops it.
    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.keys_filter.typing = false,
            KeyCode::Esc => self.keys_filter = KeyFilter::default(),
            KeyCode::Backspace => {
                self.keys_filter.text.pop();
                self.keys_scroll = 0;
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.keys_filter.text.push(c);
                self.keys_scroll = 0;
            }
            _ => {}
        }
    }

    /// `/` starts a filter; `Esc` clears a kept one, else it and `?` close; `j`/`k`
    /// scroll; every other key does nothing.
    pub(super) fn keys_key(&mut self, key: KeyEvent) {
        if self.keys_filter.typing {
            return self.filter_key(key);
        }
        match key.code {
            KeyCode::Char('/') => self.keys_filter.typing = true,
            KeyCode::Esc if !self.keys_filter.text.is_empty() => {
                self.keys_filter = KeyFilter::default();
                self.keys_scroll = 0;
            }
            KeyCode::Char('?') | KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Char('j') | KeyCode::Down => self.scroll_key_list(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_key_list(-1),
            _ => {}
        }
    }

    /// The wheel scrolls; a click outside the list closes it.
    pub(super) fn keys_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollDown => self.scroll_key_list(3),
            MouseEventKind::ScrollUp => self.scroll_key_list(-3),
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = self.geometry.keys.is_some_and(|r| {
                    mouse.column >= r.x
                        && mouse.column < r.right()
                        && mouse.row >= r.y
                        && mouse.row < r.bottom()
                });
                if !inside {
                    self.mode = Mode::Browse;
                }
            }
            _ => {}
        }
    }

    pub(super) fn draw_key_list(&mut self, frame: &mut Frame) {
        let area = frame.area();
        // Sized from the whole list so the box keeps its width while a filter is typed.
        let content = self.key_list_lines(&KeyFilter::default()).iter().map(Line::width).max().unwrap_or(0);
        let help = self.keys_filter.help();
        let content = content.max(help.width());
        let lines = match self.key_list_lines(&self.keys_filter) {
            lines if lines.is_empty() => vec![Line::from(Span::raw(NO_MATCH).dim())],
            lines => lines,
        };
        let width = (content as u16 + 4).min(area.width);
        let height = (lines.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let boxed = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::Cyan))
            .title(Span::styled(" keys ", Style::new().dim()))
            .title_bottom(Span::styled(help, Style::new().dim()));
        let inner = boxed.inner(rect);
        frame.render_widget(boxed, rect);
        let max_scroll = lines.len().saturating_sub(usize::from(inner.height));
        self.keys_scroll = self.keys_scroll.min(max_scroll);
        let shown: Vec<Line<'static>> =
            lines.into_iter().skip(self.keys_scroll).take(usize::from(inner.height)).collect();
        let text = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
        frame.render_widget(Paragraph::new(shown), text);
        self.geometry.keys = Some(rect);
        self.geometry.keys_max_scroll = max_scroll;
    }

    /// The rail's hint for the one thread key that applies to the selected note; empty
    /// when no session is attached, since the keys would only say so.
    pub(super) fn rail_thread_hint(&self) -> &'static str {
        if self.attached_session().is_none() {
            return "";
        }
        let entries = self.rail();
        let Some(entry) = entries.get(self.rail_cursor) else { return "" };
        match &entry.thread {
            Some(thread) => {
                match (matches!(thread.turn, Turn::Failed { .. } | Turn::Interrupted), thread.resolved) {
                    (true, false) => "enter open \u{b7} ctrl-t retry \u{b7} ctrl-r resolve \u{b7} ",
                    (true, true) => "enter open \u{b7} ctrl-t retry \u{b7} ctrl-r unresolve \u{b7} ",
                    (false, false) => "enter open \u{b7} ctrl-r resolve \u{b7} ",
                    (false, true) => "enter open \u{b7} ctrl-r unresolve \u{b7} ",
                }
            }
            None if entry.annotation.anchor.kind() == Kind::Comment => "ctrl-t thread \u{b7} ",
            None => "",
        }
    }
}
