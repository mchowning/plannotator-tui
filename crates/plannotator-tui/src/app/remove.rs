//! Removing notes: `x` in the rail or on a block asks before anything is deleted.

use anyhow::Result;
use plannotator_tui_schema::{Annotation, Thread};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{App, Mode};

/// What a pending `x` would remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Removal {
    /// One rail note, by annotation id.
    Note(String),
    /// Every note placed in this block.
    Block(usize),
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

    /// `x` on a block. Asks only when there is something to remove.
    pub(super) fn ask_remove_in_block(&mut self) {
        if self.notes_in_block(self.selected).is_empty() {
            self.status = Some("no notes on this block".into());
        } else {
            self.mode = Mode::ConfirmRemove(Removal::Block(self.selected));
        }
    }

    fn notes_in_block(&self, block: usize) -> Vec<&Annotation> {
        self.open
            .store
            .placed()
            .into_iter()
            .filter(|p| self.open.doc.block_containing(p.range.start) == Some(block))
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
            Removal::Block(block) => {
                let notes = self.notes_in_block(*block);
                let threads = notes.iter().filter(|a| is_thread(a)).count();
                match (notes.len(), threads) {
                    (1, 1) => "remove the thread on this block and its conversation?".to_owned(),
                    (1, _) => "remove the note on this block?".to_owned(),
                    (n, 0) => format!("remove the {n} notes on this block?"),
                    (n, 1) => format!("remove the {n} notes on this block, 1 of them a thread?"),
                    (n, t) => format!("remove the {n} notes on this block, {t} of them threads?"),
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
            Removal::Block(block) => {
                let removed = self.open.store.remove_in_block(&self.open.doc, *block)?;
                if removed > 0 {
                    self.mark_unsent();
                    self.sync_tree_counts();
                }
                self.status = Some(format!("removed {removed} annotation(s) on block"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
