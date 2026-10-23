//! The general comment: `C` writes one note about the whole document, or reopens it.

use anyhow::Result;

use super::compose::Compose;
use super::{App, Mode};

impl App {
    /// `C`: the comment box, holding the general comment when there is one.
    pub(super) fn open_general(&mut self) {
        let body = self.open.store.general().map(|a| a.body.clone()).unwrap_or_default();
        self.compose = Compose::with_text(&body);
        self.mode = Mode::General;
    }

    /// Enter in the box. Empty leaves the general comment as it was.
    pub(super) fn save_general(&mut self) -> Result<()> {
        let body = self.compose.value().trim().to_owned();
        self.mode = Mode::Browse;
        if body.is_empty() {
            self.status = Some("general comment cancelled: empty".into());
            return Ok(());
        }
        self.open.store.set_general(&self.open.doc, body)?;
        self.mark_unsent();
        self.sync_tree_counts();
        self.status = Some("general comment saved".into());
        Ok(())
    }

    pub(super) fn is_general(&self, id: &str) -> bool {
        self.open.store.general().is_some_and(|g| g.id == id)
    }
}

#[cfg(test)]
mod tests;
