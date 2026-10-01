//! `/` search over the document's rendered text, as in vim with `incsearch`: while the
//! query is typed in the footer the cursor jumps to its first match from where `/` was
//! pressed; `Enter` keeps it, `Esc` goes back. `n`/`N` step forward and back, wrapping at
//! the ends. Notes are never searched. Matches stay highlighted until `Esc`.

use std::collections::HashSet;
use std::ops::Range;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::selection::Selection;
use super::{App, Mode, Pending};

#[derive(Debug, Default)]
pub(super) struct Search {
    /// What has been typed after `/` so far.
    pub(super) typed: String,
    /// The last query entered; `n`/`N` look for it again.
    last: Option<String>,
    /// Whether matches of `last` are drawn; `Esc` turns it off, the next search back on.
    highlight: bool,
    /// Where the review was when `/` was pressed; each typed key searches from here.
    origin: Option<Origin>,
}

/// What a live search moves, kept so `Esc` can put it back.
#[derive(Debug)]
struct Origin {
    cursor: (usize, usize),
    selected: usize,
    scroll: usize,
    selection: Option<Selection>,
    pending: Option<Pending>,
    highlight: bool,
    status: Option<String>,
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
        self.search.origin = Some(Origin {
            cursor: self.cursor,
            selected: self.selected,
            scroll: self.scroll,
            selection: self.selection,
            pending: self.pending.clone(),
            highlight: self.search.highlight,
            status: self.status.clone(),
        });
        self.mode = Mode::Search;
    }

    /// Keys while typing the query. An empty `Enter` repeats the last search, as in vim.
    pub(super) fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_search(),
            KeyCode::Backspace if self.search.typed.is_empty() => self.cancel_search(),
            KeyCode::Backspace => {
                self.search.typed.pop();
                self.search_as_typed();
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.return_to_origin();
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
                self.search_as_typed();
            }
            _ => {}
        }
    }

    fn cancel_search(&mut self) {
        self.mode = Mode::Browse;
        self.return_to_origin();
    }

    /// Put back what the live search moved. The origin stays for the next key.
    fn return_to_origin(&mut self) {
        let Some(origin) = &self.search.origin else { return };
        self.cursor = origin.cursor;
        self.selected = origin.selected;
        self.scroll = origin.scroll;
        self.selection = origin.selection;
        self.pending = origin.pending.clone();
        self.search.highlight = origin.highlight;
        self.status = origin.status.clone();
    }

    /// From where `/` was pressed, move to the first match of what is typed so far.
    fn search_as_typed(&mut self) {
        self.return_to_origin();
        let positions = self.match_positions(&self.search.typed);
        let here = self.cursor;
        if let Some(&target) = positions.iter().find(|&&p| p >= here).or(positions.first()) {
            self.move_to_match(target);
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
        self.move_to_match(target);
        let index = positions.iter().position(|&p| p == target).map_or(0, |i| i + 1);
        let wrapped = if wrapped { " · wrapped" } else { "" };
        self.status = Some(format!("/{query} · {index} of {}{wrapped}", positions.len()));
    }

    fn move_to_match(&mut self, target: (usize, usize)) {
        self.clear_selection();
        if let Some(block) = self.open.layout.block_at_row(target.0) {
            self.selected = block;
        }
        self.cursor = target;
        self.ensure_cursor_visible();
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
        let query = match &self.search.last {
            _ if self.mode == Mode::Search => &self.search.typed,
            Some(last) if self.search.highlight => last,
            _ => return HashSet::new(),
        };
        let layout = &self.open.layout;
        let first = rows.clone().find_map(|r| layout.block_at_row(r));
        let last = rows.rev().find_map(|r| layout.block_at_row(r));
        let (Some(first), Some(last)) = (first, last) else { return HashSet::new() };
        (first..=last).flat_map(|block| layout.hits(block, query)).flatten().collect()
    }
}

#[cfg(test)]
mod tests;
