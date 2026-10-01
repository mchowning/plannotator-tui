//! `/` search over the document's rendered text, as in vim: type a query in the footer,
//! `Enter` jumps to the first match from the cursor, `n`/`N` step forward and back,
//! wrapping at the ends. Notes are never searched. Matches stay highlighted until `Esc`.

use std::collections::HashSet;
use std::ops::Range;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Mode};

#[derive(Debug, Default)]
pub(super) struct Search {
    /// What has been typed after `/` so far.
    pub(super) typed: String,
    /// The last query entered; `n`/`N` look for it again.
    last: Option<String>,
    /// Whether matches of `last` are drawn; `Esc` turns it off, the next search back on.
    highlight: bool,
}

/// Where to look from the cursor.
#[derive(Debug, Clone, Copy)]
pub(super) enum Seek {
    /// A new query: a match at the cursor counts. In block mode the cursor sits unseen
    /// at the block's start, and a match there is the one the reader expects.
    FromCursor,
    Next,
    Previous,
}

impl App {
    pub(super) fn start_search(&mut self) {
        self.search.typed.clear();
        self.mode = Mode::Search;
    }

    /// Keys while typing the query. An empty `Enter` repeats the last search, as in vim.
    pub(super) fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Backspace if self.search.typed.is_empty() => self.mode = Mode::Browse,
            KeyCode::Backspace => {
                self.search.typed.pop();
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                let typed = std::mem::take(&mut self.search.typed);
                if typed.is_empty() {
                    self.jump_to_match(Seek::Next);
                } else {
                    self.search.last = Some(typed);
                    self.jump_to_match(Seek::FromCursor);
                }
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.search.typed.push(c);
            }
            _ => {}
        }
    }

    /// Whether `n`/`N` have a query to look for.
    pub(super) fn has_search(&self) -> bool {
        self.search.last.is_some()
    }

    /// `Esc` in block mode: true when it turned the highlight off, so it should not also quit.
    pub(super) fn clear_search_highlight(&mut self) -> bool {
        let was = std::mem::take(&mut self.search.highlight);
        if was {
            self.status = None;
        }
        was
    }

    /// Move the cursor, and the block selection with it, to a match of the last query.
    pub(super) fn jump_to_match(&mut self, seek: Seek) {
        let Some(query) = self.search.last.clone() else { return };
        self.search.highlight = true;
        let positions = self.match_positions(&query);
        let here = self.cursor;
        let ahead = match seek {
            Seek::FromCursor => positions.iter().find(|&&p| p >= here),
            Seek::Next => positions.iter().find(|&&p| p > here),
            Seek::Previous => positions.iter().rev().find(|&&p| p < here),
        };
        let wrapped = match seek {
            Seek::Previous => positions.last(),
            Seek::FromCursor | Seek::Next => positions.first(),
        };
        let Some((&target, wrapped)) = ahead.map(|p| (p, false)).or(wrapped.map(|p| (p, true))) else {
            self.status = Some(format!("not found: {query}"));
            return;
        };
        self.clear_selection();
        if let Some(block) = self.open.layout.block_at_row(target.0) {
            self.selected = block;
        }
        self.cursor = target;
        self.ensure_cursor_visible();
        let index = positions.iter().position(|&p| p == target).map_or(0, |i| i + 1);
        let wrapped = if wrapped { " · wrapped" } else { "" };
        self.status = Some(format!("/{query} · {index} of {}{wrapped}", positions.len()));
    }

    /// Where each match starts on screen, in document order.
    fn match_positions(&self, query: &str) -> Vec<(usize, usize)> {
        let layout = &self.open.layout;
        let mut positions: Vec<(usize, usize)> = (0..layout.blocks.len())
            .flat_map(|block| {
                layout
                    .hits(block, query)
                    .into_iter()
                    .filter_map(move |hit| layout.position_of(block, *hit.first()?))
            })
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    /// Source offsets to draw as matches in document rows `rows`. Only the blocks on
    /// screen are searched, so a frame costs what it shows.
    pub(super) fn search_highlights(&self, rows: Range<usize>) -> HashSet<usize> {
        let (true, Some(query)) = (self.search.highlight, &self.search.last) else { return HashSet::new() };
        let layout = &self.open.layout;
        let first = rows.clone().find_map(|r| layout.block_at_row(r));
        let last = rows.rev().find_map(|r| layout.block_at_row(r));
        let (Some(first), Some(last)) = (first, last) else { return HashSet::new() };
        (first..=last).flat_map(|block| layout.hits(block, query)).flatten().collect()
    }
}

#[cfg(test)]
mod tests;
