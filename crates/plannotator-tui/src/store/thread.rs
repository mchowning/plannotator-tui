//! Thread changes and the send handoff. Each runs as one locked read-modify-write, so the
//! review UI and the `thread` CLI decide against the record as it is now, not as it was
//! when they loaded it.

use std::collections::BTreeMap;
use std::ops::Range;

use anyhow::{Result, bail};
use plannotator_tui_schema::{
    AGENT, Annotation, Fork, Kind, Reply, Resolution, Thread, ThreadState, Turn, USER,
};

use super::{Record, Store, local_id, new_annotation, timestamp};
use crate::doc::Document;

/// A thread in the record, with whether its passage still resolves.
#[derive(Debug)]
pub(crate) struct ThreadEntry<'a> {
    pub(crate) annotation: &'a Annotation,
    pub(crate) thread: Thread,
    pub(crate) detached: bool,
}

/// What `Ctrl-R` on an existing annotation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThreadKey {
    /// A regular comment became a thread.
    Started,
    /// A failed or interrupted turn will run again.
    Retried,
    /// A thread with nothing to retry.
    AlreadyThread,
    /// The annotation is gone from the record.
    Missing,
}

fn live_thread<'a>(record: &'a mut Record, id: &str) -> Result<(&'a mut Annotation, Thread)> {
    let Some(annotation) = record.annotations.iter_mut().find(|a| a.id == id) else {
        bail!("no annotation {id} in the record")
    };
    match Thread::of(annotation)? {
        Some(thread) if thread.state == ThreadState::Live => Ok((annotation, thread)),
        Some(_) => bail!("thread {id} is historical; it is read-only"),
        None => bail!("annotation {id} is not a thread"),
    }
}

fn reply(annotation_id: &str, author: &str, body: String) -> Result<Reply> {
    let now = timestamp()?;
    Ok(Reply {
        id: local_id(),
        annotation_id: annotation_id.to_owned(),
        body,
        author: Some(author.to_owned()),
        author_name: None,
        created_at: now.clone(),
        updated_at: now,
        other: BTreeMap::new(),
    })
}

impl Store {
    /// Every active annotation that is a thread, in record order.
    pub(crate) fn threads(&self) -> Result<Vec<ThreadEntry<'_>>> {
        let mut out = Vec::new();
        for (annotation, resolution) in self.annotations.iter().zip(&self.resolved) {
            if let Some(thread) = Thread::of(annotation)? {
                out.push(ThreadEntry { annotation, thread, detached: *resolution == Resolution::Orphan });
            }
        }
        Ok(out)
    }

    /// Save a comment on `range` that is a thread from the start.
    pub(crate) fn add_thread(
        &mut self,
        doc: &Document,
        range: Range<usize>,
        rendered: String,
        body: String,
    ) -> Result<String> {
        let mut annotation = new_annotation(doc, range, rendered, Kind::Comment, body)?;
        Thread::default().store_on(&mut annotation)?;
        let id = annotation.id.clone();
        self.mutate(Some(doc), |record| {
            record.annotations.push(annotation);
            Ok(())
        })?;
        Ok(id)
    }

    /// `Ctrl-R` on an annotation: turn a regular comment into a thread, or retry a thread
    /// whose last turn failed or was interrupted.
    pub(crate) fn thread_key(&mut self, id: &str) -> Result<ThreadKey> {
        self.mutate(None, |record| {
            let Some(annotation) = record.annotations.iter_mut().find(|a| a.id == id) else {
                return Ok(ThreadKey::Missing);
            };
            let Some(mut thread) = Thread::of(annotation)? else {
                Thread::default().store_on(annotation)?;
                return Ok(ThreadKey::Started);
            };
            let retryable = matches!(thread.turn, Turn::Failed { .. } | Turn::Interrupted);
            if thread.state != ThreadState::Live || !retryable {
                return Ok(ThreadKey::AlreadyThread);
            }
            thread.turn = Turn::Idle;
            thread.store_on(annotation)?;
            Ok(ThreadKey::Retried)
        })
    }

    /// The person's reply in a live thread; it unresolves the thread. Returns the reply id.
    pub(crate) fn add_user_reply(&mut self, id: &str, body: String) -> Result<String> {
        self.mutate(None, |record| {
            let (annotation, mut thread) = live_thread(record, id)?;
            if thread.resolved {
                thread.resolved = false;
                thread.store_on(annotation)?;
            }
            let reply = reply(id, USER, body)?;
            let reply_id = reply.id.clone();
            annotation.replies.push(reply);
            Ok(reply_id)
        })
    }

    /// Link a live thread to the fork that answers it.
    pub(crate) fn set_fork(&mut self, id: &str, fork: Fork) -> Result<()> {
        self.mutate(None, |record| {
            let (annotation, mut thread) = live_thread(record, id)?;
            thread.fork = Some(fork);
            thread.store_on(annotation)?;
            Ok(())
        })
    }

    /// Mark a live thread resolved, or not.
    pub(crate) fn set_resolved(&mut self, id: &str, resolved: bool) -> Result<()> {
        self.mutate(None, |record| {
            let (annotation, mut thread) = live_thread(record, id)?;
            thread.resolved = resolved;
            thread.store_on(annotation)?;
            Ok(())
        })
    }

    pub(crate) fn set_turn(&mut self, id: &str, turn: Turn) -> Result<()> {
        self.mutate(None, |record| {
            let (annotation, mut thread) = live_thread(record, id)?;
            thread.turn = turn;
            thread.store_on(annotation)?;
            Ok(())
        })
    }

    /// The fork's answer to every user message up to and including `through`. Ends the
    /// turn. Returns the reply id.
    pub(crate) fn add_agent_reply(&mut self, id: &str, through: &str, body: String) -> Result<String> {
        self.mutate(None, |record| {
            let (annotation, mut thread) = live_thread(record, id)?;
            if !thread.unanswered(annotation).iter().any(|m| m.id == through) {
                bail!("{through} is not an unanswered user message in thread {id}");
            }
            thread.answered_through = Some(through.to_owned());
            thread.turn = Turn::Idle;
            thread.store_on(annotation)?;
            let reply = reply(id, AGENT, body)?;
            let reply_id = reply.id.clone();
            annotation.replies.push(reply);
            Ok(reply_id)
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]
mod tests {
    use super::*;
    use crate::store::Location;

    fn fixture(tag: &str) -> (std::path::PathBuf, Location, Document, Store) {
        let root =
            std::env::temp_dir().join(format!("plannotator-store-thread-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let location = Location::for_file(&root.join("data"), "project", &root.join("plan.md"));
        let doc = Document::parse("one two three\n".to_owned());
        let store = Store::load(&location, &doc).expect("empty store");
        (root, location, doc, store)
    }

    fn thread(store: &Store, id: &str) -> Option<Thread> {
        let annotation = store.annotations.iter().find(|a| a.id == id)?;
        Thread::of(annotation).expect("readable")
    }

    #[test]
    fn ctrl_r_starts_a_thread_retries_a_broken_turn_and_leaves_a_healthy_one_alone() {
        let (root, _, doc, mut store) = fixture("ctrl-r");
        store.add(&doc, 0..3, "one".into(), Kind::Comment, "why?".into()).expect("comment");
        let id = store.annotations[0].id.clone();
        assert_eq!(store.thread_key(&id).expect("start"), ThreadKey::Started);
        assert_eq!(thread(&store, &id), Some(Thread::default()), "the body is the first message");
        assert_eq!(store.thread_key(&id).expect("healthy"), ThreadKey::AlreadyThread);

        for broken in
            [Turn::Failed { detail: "rate limited (429)".into(), retryable: true }, Turn::Interrupted]
        {
            store.set_turn(&id, broken).expect("break");
            assert_eq!(store.thread_key(&id).expect("retry"), ThreadKey::Retried);
            assert_eq!(thread(&store, &id).expect("thread").turn, Turn::Idle, "the runner picks it up again");
        }
        assert_eq!(store.thread_key("gone").expect("missing"), ThreadKey::Missing);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_thread_saved_from_compose_is_on_disk_with_its_first_message() {
        let (root, location, doc, mut store) = fixture("compose");
        let id = store.add_thread(&doc, 4..7, "two".into(), "what is two?".into()).expect("thread");
        let reloaded = Store::load(&location, &doc).expect("reload");
        assert_eq!(reloaded.annotations[0].body, "what is two?");
        assert!(thread(&reloaded, &id).expect("thread").needs_turn(&reloaded.annotations[0]));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_resolved_thread_stays_resolved_after_a_reload_and_still_takes_turns() {
        let (root, location, doc, mut store) = fixture("resolve");
        let id = store.add_thread(&doc, 0..3, "one".into(), "first".into()).expect("thread");
        store.set_resolved(&id, true).expect("resolve");
        let reloaded = Store::load(&location, &doc).expect("reload");
        let t = thread(&reloaded, &id).expect("thread");
        assert!(t.resolved);
        assert!(t.needs_turn(&reloaded.annotations[0]), "resolving only changes how it looks");
        store.set_resolved(&id, false).expect("unresolve");
        assert!(!thread(&store, &id).expect("thread").resolved);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_user_reply_unresolves_a_thread_and_an_agent_reply_does_not() {
        let (root, _, doc, mut store) = fixture("reply-unresolves");
        let id = store.add_thread(&doc, 0..3, "one".into(), "first".into()).expect("thread");
        store.set_resolved(&id, true).expect("resolve");
        store.add_agent_reply(&id, &id, "answer".into()).expect("answer");
        assert!(thread(&store, &id).expect("thread").resolved, "the agent finishing a turn leaves it");
        store.add_user_reply(&id, "one more thing".into()).expect("reply");
        assert!(!thread(&store, &id).expect("thread").resolved, "a new question reopens it");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_user_reply_queues_behind_a_running_turn_and_a_historical_thread_takes_none() {
        let (root, _, doc, mut store) = fixture("reply");
        let id = store.add_thread(&doc, 0..3, "one".into(), "first".into()).expect("thread");
        store.set_turn(&id, Turn::Running).expect("running");
        let queued = store.add_user_reply(&id, "second".into()).expect("queued while running");
        store.add_agent_reply(&id, &id, "answer to first".into()).expect("answer");
        let annotation = &store.annotations[0];
        let t = thread(&store, &id).expect("thread");
        assert!(t.needs_turn(annotation), "the queued reply gets its own turn");
        assert_eq!(t.unanswered(annotation)[0].id, queued);

        let mut historical = t;
        historical.state = ThreadState::Historical;
        store
            .mutate(None, |record| historical.store_on(&mut record.annotations[0]).map_err(Into::into))
            .expect("make historical");
        assert!(store.add_user_reply(&id, "third".into()).is_err(), "a transcript is read-only");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
