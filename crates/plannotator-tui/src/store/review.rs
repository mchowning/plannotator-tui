//! Delivery coverage and recoverable finished reviews. Changes commit to memory only
//! after the record has been saved, so a failed archive or restore leaves it intact.

use std::collections::HashSet;

use anyhow::{Context, Result};
use plannotator_tui_schema::{Annotation, Thread, ThreadState, Turn};
use time::{Duration, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use super::{Delivered, Record, Store};
use crate::doc::Document;

/// Parsing stays lenient: any RFC 3339 offset and sub-second precision is accepted.
fn parse_time(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

/// Every timestamp the record writes has one shape, `YYYY-MM-DDTHH:MM:SS.mmmZ`, so a
/// record never mixes precisions (see `archive::iso_millis`). Finer digits are dropped.
fn format_millis(at: OffsetDateTime) -> Result<String> {
    let at = at.checked_to_offset(UtcOffset::UTC).context("annotation time out of range")?;
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.millisecond()
    ))
}

/// The millisecond at or after `at`, so a stored copy never sorts before the instant it
/// stands for when that instant carried finer digits.
fn ceil_millis(at: OffsetDateTime) -> Result<OffsetDateTime> {
    let below = i64::from(at.nanosecond() % 1_000_000);
    if below == 0 {
        return Ok(at);
    }
    at.checked_add(Duration::nanoseconds(1_000_000 - below)).context("advancing annotation time")
}

pub(super) fn timestamp() -> Result<String> {
    format_millis(OffsetDateTime::now_utc())
}

fn last_delivery<'a>(deliveries: &'a [Delivered], id: &str) -> Option<&'a Delivered> {
    deliveries.iter().rev().find(|d| d.annotation_ids.iter().any(|sent| sent == id))
}

/// Coverage belongs to each annotation's last successful send, not the last batch.
/// A delivery whose time cannot be read covers nothing, so the annotation stays
/// pending rather than silently hiding feedback. An annotation whose own `updated_at`
/// cannot be read is the other way round: once it has been in any delivery it counts as
/// sent, because "pending forever" would resend it on every send and never let it be
/// archived. The next edit rewrites the timestamp and makes it pending again.
pub(super) fn pending(deliveries: &[Delivered], annotation: &Annotation) -> bool {
    let Some(delivery) = last_delivery(deliveries, &annotation.id) else { return true };
    match (parse_time(&annotation.updated_at), parse_time(&delivery.at)) {
        (Some(updated), Some(sent)) => updated > sent,
        (None, _) => false,
        (Some(_), None) => true,
    }
}

impl Store {
    /// Compare persisted review data, independent of resolution against the document.
    pub(crate) fn same_review(&self, other: &Self) -> bool {
        self.annotations == other.annotations
            && self.deliveries == other.deliveries
            && self.archived == other.archived
    }

    pub(crate) fn is_pending(&self, annotation: &Annotation) -> bool {
        pending(&self.deliveries, annotation)
    }

    pub(crate) fn all_delivered(&self) -> bool {
        !self.annotations.is_empty() && self.annotations.iter().all(|a| !self.is_pending(a))
    }

    /// Replace a body, advancing the existing timestamp beyond its last send even when
    /// the clock has not ticked (or has moved backwards) since that send. One millisecond
    /// is the smallest step the stored shape can represent.
    pub(crate) fn edit_body(&mut self, id: &str, body: String) -> Result<bool> {
        self.mutate(None, |record| {
            let previous = last_delivery(&record.deliveries, id).and_then(|d| parse_time(&d.at));
            let Some(annotation) = record.annotations.iter_mut().find(|a| a.id == id) else {
                return Ok(false);
            };
            if annotation.body == body {
                return Ok(false);
            }
            let previous = [parse_time(&annotation.updated_at), previous].into_iter().flatten().max();
            let mut now = OffsetDateTime::now_utc();
            if let Some(previous) = previous {
                now = now.max(
                    previous.checked_add(Duration::milliseconds(1)).context("advancing annotation time")?,
                );
            }
            annotation.updated_at = format_millis(now)?;
            annotation.body = body;
            Ok(true)
        })
    }

    /// Record only ids that actually appeared in the delivered feedback body.
    pub(crate) fn record_delivery(&mut self, target: &str, annotation_ids: &[String]) -> Result<()> {
        if annotation_ids.is_empty() {
            return Ok(());
        }
        self.mutate(None, |record| {
            record.deliveries.push(delivery(&record.annotations, target, annotation_ids)?);
            Ok(())
        })
    }

    pub(crate) fn archived(&self) -> &[Annotation] {
        &self.archived
    }

    /// Finish delivered, unchanged annotations, including ones whose quote is now gone.
    /// Pending annotations stay active. The returned ids are the undo operation.
    pub(crate) fn archive_sent(&mut self) -> Result<Vec<String>> {
        self.mutate(None, |record| {
            let ids: Vec<String> = record
                .annotations
                .iter()
                .filter(|a| !pending(&record.deliveries, a))
                .map(|a| a.id.clone())
                .collect();
            archive(record, &ids)?;
            Ok(ids)
        })
    }

    /// Restore without editing timestamps or delivery history. A conflicting active id
    /// is left alone, with the archived copy retained for recovery.
    pub(crate) fn restore_archived(&mut self, doc: &Document, ids: &[String]) -> Result<usize> {
        self.mutate(Some(doc), |record| {
            let archived = std::mem::take(&mut record.archived);
            let mut restored = 0;
            for annotation in archived {
                if ids.contains(&annotation.id) && !record.annotations.iter().any(|a| a.id == annotation.id) {
                    record.annotations.push(annotation);
                    restored += 1;
                } else {
                    record.archived.push(annotation);
                }
            }
            Ok(restored)
        })
    }
}

/// A delivery of `annotation_ids`, timed no earlier than the newest of their edits.
pub(super) fn delivery(
    annotations: &[Annotation],
    target: &str,
    annotation_ids: &[String],
) -> Result<Delivered> {
    let updated = annotations
        .iter()
        .filter(|a| annotation_ids.contains(&a.id))
        .filter_map(|a| parse_time(&a.updated_at))
        .max();
    let now = updated.map_or_else(OffsetDateTime::now_utc, |at| at.max(OffsetDateTime::now_utc()));
    Ok(Delivered {
        at: format_millis(ceil_millis(now)?)?,
        target: target.to_owned(),
        annotation_ids: annotation_ids.to_vec(),
    })
}

/// Move the active annotations named by `ids` into the archive. An archived thread is a
/// transcript: historical, with no fork to answer it and no turn running.
pub(super) fn archive(record: &mut Record, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let selected: HashSet<&str> = ids.iter().map(String::as_str).collect();
    anyhow::ensure!(
        !record.archived.iter().any(|a| selected.contains(a.id.as_str())),
        "cannot archive: an annotation with the same id is already archived"
    );
    let (finished, active): (Vec<Annotation>, Vec<Annotation>) =
        std::mem::take(&mut record.annotations).into_iter().partition(|a| selected.contains(a.id.as_str()));
    record.annotations = active;
    for mut annotation in finished {
        if let Some(mut thread) = Thread::of(&annotation)? {
            thread.state = ThreadState::Historical;
            thread.fork = None;
            if thread.turn == Turn::Running {
                thread.turn = Turn::Interrupted;
            }
            thread.store_on(&mut annotation)?;
        }
        record.archived.push(annotation);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
