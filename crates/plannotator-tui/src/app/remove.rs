//! Removing notes: `x` in the rail, on a block or on a table row asks before anything is
//! deleted.

use std::ops::Range;

use anyhow::Result;
use plannotator_tui_schema::{Annotation, Thread};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{App, Mode};

/// What a pending `x` would remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Removal {
    /// One rail note, by annotation id.
    Note(String),
    /// Every note that starts in this source range.
    Within(Range<usize>, Scope),
}

/// What `x` in the document was pressed on, for the question and the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Block,
    TableRow,
}

impl Scope {
    fn noun(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::TableRow => "row",
        }
    }
}

fn is_thread(annotation: &Annotation) -> bool {
    Thread::of(annotation).ok().flatten().is_some()
}

impl App {
    /// `x` or `Del` in the rail.
    pub(super) fn ask_remove_selected_note(&mut self) {
        if let Some(id) = self.rail_selected_id() {
            self.mode = Mode::ConfirmRemove(Removal::Note(id));
        }
    }

    /// `x` on the selected table row, else the selected block. Asks only when there is
    /// something to remove.
    pub(super) fn ask_remove_in_block(&mut self) {
        let (range, scope) = match self.selected_table_row() {
            Some((_, range)) => (range, Scope::TableRow),
            None => match self.open.doc.blocks.get(self.selected) {
                Some(block) => (block.range.clone(), Scope::Block),
                None => return,
            },
        };
        if self.notes_within(&range).is_empty() {
            self.status = Some(format!("no notes on this {}", scope.noun()));
        } else {
            self.mode = Mode::ConfirmRemove(Removal::Within(range, scope));
        }
    }

    fn notes_within(&self, range: &Range<usize>) -> Vec<&Annotation> {
        self.open
            .store
            .placed()
            .into_iter()
            .filter(|p| range.contains(&p.range.start))
            .map(|p| p.annotation)
            .collect()
    }

    /// The footer's question for `removal`.
    pub(super) fn removal_question(&self, removal: &Removal) -> String {
        let question = match removal {
            Removal::Note(id) => {
                let thread = self.rail().iter().any(|e| &e.annotation.id == id && e.thread.is_some());
                if thread {
                    "remove this thread and its conversation?".to_owned()
                } else {
                    "remove this note?".to_owned()
                }
            }
            Removal::Within(range, scope) => {
                let notes = self.notes_within(range);
                let threads = notes.iter().filter(|a| is_thread(a)).count();
                let on = scope.noun();
                match (notes.len(), threads) {
                    (1, 1) => format!("remove the thread on this {on} and its conversation?"),
                    (1, _) => format!("remove the note on this {on}?"),
                    (n, 0) => format!("remove the {n} notes on this {on}?"),
                    (n, 1) => format!("remove the {n} notes on this {on}, 1 of them a thread?"),
                    (n, t) => format!("remove the {n} notes on this {on}, {t} of them threads?"),
                }
            }
        };
        format!(" {question} y remove \u{b7} n keep")
    }

    /// `y` or `Enter` removes; `n` or `Esc` keeps; every other key does nothing.
    pub(super) fn confirm_remove_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                match std::mem::replace(&mut self.mode, Mode::Browse) {
                    Mode::ConfirmRemove(removal) => self.remove(&removal),
                    _ => Ok(()),
                }
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.mode = Mode::Browse;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn remove(&mut self, removal: &Removal) -> Result<()> {
        match removal {
            Removal::Note(id) => {
                if self.open.store.remove(id)? {
                    self.mark_unsent();
                    self.status = Some("annotation removed".into());
                    self.clamp_rail_cursor();
                    self.sync_tree_counts();
                }
            }
            Removal::Within(range, scope) => {
                let removed = self.open.store.remove_starting_in(&self.open.doc, range)?;
                if removed > 0 {
                    self.mark_unsent();
                    self.sync_tree_counts();
                }
                self.status = Some(format!("removed {removed} annotation(s) on {}", scope.noun()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
