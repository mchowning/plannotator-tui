//! Unread notes: `u` on the rail marks one, opening a thread reads it, and a thread open in
//! the panel stays read while its answers land.

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
