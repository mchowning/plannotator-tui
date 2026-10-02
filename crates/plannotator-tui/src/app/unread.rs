//! Unread notes: `u` on the rail marks one, `n`/`N` step through them, opening a thread
//! reads it, and a thread open in the panel stays read while its answers land.

use anyhow::Result;
use plannotator_tui_schema::{ThreadState, is_unread};

use super::{App, Mode};

impl App {
    /// `u` on the rail: mark the selected note unread, or read. A thread already sent is
    /// a transcript, with nothing to come back to.
    pub(super) fn toggle_unread_on_rail(&mut self) -> Result<()> {
        let entries = self.rail();
        let Some(entry) = entries.get(self.rail_cursor) else { return Ok(()) };
        let id = entry.annotation.id.clone();
        let unread = is_unread(entry.annotation);
        let historical = entry.thread.as_ref().is_some_and(|t| t.state == ThreadState::Historical);
        if historical {
            self.status = Some("read-only: this thread was already sent".into());
            return Ok(());
        }
        self.open.store.set_unread(&id, !unread)?;
        self.status = Some(if unread { "marked read" } else { "marked unread" }.into());
        Ok(())
    }

    /// `n`/`N` on the rail: the cursor to the next or previous unread note, wrapping. Only
    /// moves; opening it is what reads it.
    pub(super) fn step_to_unread(&mut self, forward: bool) {
        let entries = self.rail();
        let len = entries.len();
        let at = self.rail_cursor;
        let found = (1..=len)
            .map(|step| if forward { (at + step) % len } else { (at + len - step) % len })
            .find(|&i| entries.get(i).is_some_and(|e| is_unread(e.annotation)));
        match found {
            Some(i) => self.rail_cursor = i,
            None => self.status = Some("no unread".into()),
        }
    }

    pub(super) fn unread_count(&self) -> usize {
        self.rail().iter().filter(|e| is_unread(e.annotation)).count()
    }

    /// The thread open in the panel, read: on opening, and again whenever a turn ends while
    /// it is on screen.
    pub(super) fn read_open_thread(&mut self) -> Result<()> {
        let Mode::Thread(id) = &self.mode else { return Ok(()) };
        let threads = self.open.store.threads()?;
        let unread_thread = threads.iter().any(|t| &t.annotation.id == id && is_unread(t.annotation));
        if unread_thread {
            self.open.store.set_unread(id, false)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
