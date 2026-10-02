//! The unread flag on an active annotation. Turns that end mark their thread unread in
//! `thread.rs`; archiving clears it in `review.rs`.

use anyhow::{Result, bail};
use plannotator_tui_schema::set_unread;

use super::Store;

impl Store {
    /// Mark an active annotation unread, or read. Changes how it is shown, nothing else:
    /// the annotation is no more or less pending for it.
    pub(crate) fn set_unread(&mut self, id: &str, unread: bool) -> Result<()> {
        self.mutate(None, |record| {
            let Some(annotation) = record.annotations.iter_mut().find(|a| a.id == id) else {
                bail!("no annotation {id} in the record")
            };
            set_unread(annotation, unread);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
