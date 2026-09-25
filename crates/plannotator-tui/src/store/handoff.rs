//! Sending an attached review: the body, its delivery record, and the archive commit in one
//! locked write, with the body left in the record for the pi session that sent it.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::review::{archive, delivery, pending};
use super::{Store, local_id, timestamp};
use crate::doc::Document;

/// A sent review waiting for delivery by the pi session that sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Handoff {
    pub(crate) id: String,
    /// The pi session that sent it, and the only one that may deliver it.
    pub(crate) owner: String,
    pub(crate) body: String,
    pub(crate) created_at: String,
}

impl Store {
    /// Send the review as the record stands under the lock. `render` gets the record
    /// resolved against `doc` and returns the body and the ids it covers. In the same
    /// write: the handoff for `owner`, a delivery to `target`, and the archive of everything
    /// delivered. A thread reply that lands first is in the body; one that lands after
    /// finds its thread archived and fails.
    pub(crate) fn hand_off(
        &mut self,
        doc: &Document,
        owner: &str,
        target: &str,
        render: impl FnOnce(&Self) -> (String, Vec<String>),
    ) -> Result<Handoff> {
        let mut view = self.clone();
        self.mutate(Some(doc), |record| {
            if let Some(waiting) = &record.handoff {
                bail!("an undelivered review from pi session {} is waiting", waiting.owner);
            }
            view.adopt(record.clone(), Some(doc));
            let (body, ids) = render(&view);
            if ids.is_empty() {
                bail!("nothing to send");
            }
            record.deliveries.push(delivery(&record.annotations, target, &ids)?);
            let delivered: Vec<String> = record
                .annotations
                .iter()
                .filter(|a| !pending(&record.deliveries, a))
                .map(|a| a.id.clone())
                .collect();
            archive(record, &delivered)?;
            let handoff = Handoff { id: local_id(), owner: owner.to_owned(), body, created_at: timestamp()? };
            record.handoff = Some(handoff.clone());
            Ok(handoff)
        })
    }

    /// The pending handoff, read under the lock.
    pub(crate) fn handoff(&mut self) -> Result<Option<Handoff>> {
        self.mutate(None, |record| Ok(record.handoff.clone()))
    }

    /// Clear the pending handoff once its owner has delivered it.
    pub(crate) fn ack_handoff(&mut self, handoff_id: &str) -> Result<()> {
        self.mutate(None, |record| match &record.handoff {
            Some(handoff) if handoff.id == handoff_id => {
                record.handoff = None;
                Ok(())
            }
            Some(handoff) => bail!("the pending handoff is {}, not {handoff_id}", handoff.id),
            None => bail!("no pending handoff"),
        })
    }
}

#[cfg(test)]
mod tests;
