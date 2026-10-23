//! The general comment: `C` writes one note about the whole document, or reopens it.

use anyhow::Result;

use super::compose::Compose;
use super::{App, Mode};

impl App {
    /// `C`, or `c` at the quit question: the comment box, holding the general comment
    /// when there is one.
    pub(super) fn open_general(&mut self, then_send: bool) {
        let body = self.open.store.general().map(|a| a.body.clone()).unwrap_or_default();
        self.compose = Compose::with_text(&body);
        self.mode = Mode::General { then_send };
    }

    /// Enter in the box. Empty leaves the general comment as it was. From the quit
    /// question, a saved comment is sent with the rest, the way `y` would send it.
    pub(super) fn save_general(&mut self) -> Result<()> {
        let then_send = self.mode == Mode::General { then_send: true };
        let body = self.compose.value().trim().to_owned();
        self.mode = Mode::Browse;
        if body.is_empty() {
            self.mode = if then_send { Mode::ConfirmQuit } else { Mode::Browse };
            self.status = Some("general comment cancelled: empty".into());
            return Ok(());
        }
        self.open.store.set_general(&self.open.doc, body)?;
        self.mark_unsent();
        self.sync_tree_counts();
        self.status = Some("general comment saved".into());
        match (then_send, self.attached_session().is_some()) {
            (false, _) => Ok(()),
            (true, true) => {
                self.send_attached();
                Ok(())
            }
            (true, false) => self.send_and_quit(),
        }
    }

    pub(super) fn is_general(&self, id: &str) -> bool {
        self.open.store.general().is_some_and(|g| g.id == id)
    }
}

#[cfg(test)]
mod tests;
