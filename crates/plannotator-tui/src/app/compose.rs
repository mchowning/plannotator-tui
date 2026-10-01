//! The comment box: a small multi-line editor.
//!
//! Enter saves; Shift+Enter (where the terminal's keyboard protocol distinguishes it),
//! Alt+Enter and Ctrl+J insert a new line; Esc cancels. The buffer is a char vec with a
//! cursor, wrapped to the box width for display only — the saved body keeps exactly the
//! newlines the user typed.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// What a keystroke did to the compose box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComposeAction {
    Edited,
    Save,
    /// Ctrl-R: save as a thread.
    SaveThread,
    Cancel,
}

#[derive(Debug, Default)]
pub(super) struct Compose {
    chars: Vec<char>,
    cursor: usize,
}

fn width_of(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

impl Compose {
    pub(super) fn with_text(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub(super) fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> ComposeAction {
        match key.code {
            KeyCode::Esc => return ComposeAction::Cancel,
            KeyCode::Enter if key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) => {
                self.insert('\n');
            }
            KeyCode::Enter => return ComposeAction::Save,
            KeyCode::Char('r' | 'R') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ComposeAction::SaveThread;
            }
            KeyCode::Char('j' | 'J') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert('\n');
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Up => self.move_vertical(-1),
            KeyCode::Down => self.move_vertical(1),
            KeyCode::Home => {
                while self.cursor > 0 && self.chars.get(self.cursor - 1) != Some(&'\n') {
                    self.cursor -= 1;
                }
            }
            KeyCode::End => {
                while self.cursor < self.chars.len() && self.chars.get(self.cursor) != Some(&'\n') {
                    self.cursor += 1;
                }
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.insert(c);
            }
            _ => {}
        }
        ComposeAction::Edited
    }

    /// Insert pasted text as typed characters, newlines included.
    pub(super) fn insert_text(&mut self, text: &str) {
        for c in text.replace("\r\n", "\n").replace('\r', "\n").chars() {
            self.insert(c);
        }
    }

    fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Move to the logical line above or below, landing on the nearest display column.
    fn move_vertical(&mut self, delta: isize) {
        let text: String = self.chars.iter().take(self.cursor).collect();
        let row = text.split('\n').count().saturating_sub(1);
        let col: usize = text.rsplit('\n').next().unwrap_or("").chars().map(width_of).sum();
        let all: String = self.chars.iter().collect();
        let lines: Vec<&str> = all.split('\n').collect();
        let target = row.saturating_add_signed(delta).min(lines.len().saturating_sub(1));
        let mut cursor: usize = lines.iter().take(target).map(|l| l.chars().count() + 1).sum();
        let mut used = 0;
        for c in lines.get(target).copied().unwrap_or_default().chars() {
            let w = width_of(c);
            if used + w > col {
                break;
            }
            used += w;
            cursor += 1;
        }
        self.cursor = cursor;
    }

    /// The buffer wrapped to `width` display cells, with the cursor's (row, col) in that
    /// wrapping. Explicit newlines always break. A word that would cross the edge moves
    /// whole to the next row; only a word wider than a row is split. Spaces never wrap:
    /// they stay at the end of their row, as in a word processor.
    pub(super) fn wrapped(&self, width: usize) -> (Vec<String>, usize, usize) {
        let width = width.max(1);
        // Each row as a range of `chars`; a newline belongs to no row.
        let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
        let mut start = 0;
        let mut used = 0;
        // Where the current row could break: just after its last space.
        let mut last_break = None;
        for (i, &c) in self.chars.iter().enumerate() {
            if c == '\n' {
                rows.push(start..i);
                (start, used, last_break) = (i + 1, 0, None);
                continue;
            }
            let w = width_of(c);
            if c != ' ' && used + w > width && i > start {
                let end = last_break.unwrap_or(i);
                rows.push(start..end);
                start = end;
                used = self.width_between(end, i);
                last_break = None;
            }
            used += w;
            if c == ' ' {
                last_break = Some(i + 1);
            }
        }
        rows.push(start..self.chars.len());

        // The cursor sits on the last row starting at or before it, so a cursor on a
        // soft break shows at the start of the next row.
        let cursor_row = rows.iter().rposition(|r| r.start <= self.cursor).unwrap_or(0);
        let cursor_col = rows.get(cursor_row).map_or(0, |r| self.width_between(r.start, self.cursor));
        let lines =
            rows.into_iter().map(|r| self.chars.get(r).unwrap_or_default().iter().collect()).collect();
        (lines, cursor_row, cursor_col)
    }

    /// Display width of `chars[from..to]`.
    fn width_between(&self, from: usize, to: usize) -> usize {
        self.chars.get(from..to).unwrap_or_default().iter().copied().map(width_of).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::Compose;

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|&l| l.to_owned()).collect()
    }

    #[test]
    fn a_word_that_does_not_fit_moves_whole_to_the_next_row() {
        let compose = Compose::with_text("one two three");
        assert_eq!(compose.wrapped(10), (rows(&["one two ", "three"]), 1, 5));
    }

    #[test]
    fn a_word_longer_than_the_row_is_split_at_the_edge() {
        let compose = Compose::with_text("a abcdefghijkl");
        assert_eq!(compose.wrapped(5), (rows(&["a ", "abcde", "fghij", "kl"]), 3, 2));
    }
}
