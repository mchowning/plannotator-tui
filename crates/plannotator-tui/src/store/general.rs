//! The general comment: one note about the whole document rather than a passage in it.

use anyhow::Result;
use plannotator_tui_schema::{Anchor, Annotation, Kind};

use super::review::touch;
use super::{Store, local_id, timestamp};
use crate::doc::Document;

impl Store {
    pub(crate) fn general(&self) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.anchor.kind() == Kind::General)
    }

    /// Save `body` as the document's general comment, replacing the one already there.
    pub(crate) fn set_general(&mut self, doc: &Document, body: String) -> Result<()> {
        let now = timestamp()?;
        self.mutate(Some(doc), |record| {
            match record.annotations.iter_mut().find(|a| a.anchor.kind() == Kind::General) {
                Some(existing) if existing.body == body => {}
                Some(existing) => {
                    touch(&record.deliveries, existing)?;
                    existing.body = body;
                }
                None => record.annotations.push(Annotation {
                    id: local_id(),
                    document_id: String::new(),
                    anchor: Anchor::general(),
                    body,
                    author: None,
                    author_name: None,
                    state: plannotator_tui_schema::State::Open,
                    attachments: Vec::new(),
                    plannotator_tui: plannotator_tui_schema::NoteExtras::default(),
                    created_at: now.clone(),
                    updated_at: now,
                    replies: Vec::new(),
                    other: std::collections::BTreeMap::default(),
                }),
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
