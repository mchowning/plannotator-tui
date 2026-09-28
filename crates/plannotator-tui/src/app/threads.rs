//! Review threads: attaching an agent session, and the keys that start, retry and resolve
//! threads.

use anyhow::Result;
use plannotator_tui_schema::{Provenance, ThreadState};

use super::{App, Mode};
use crate::store::ThreadKey;

pub(super) const NOT_ATTACHED: &str = "no agent session attached";

impl App {
    pub(crate) fn attach_agent_session(&mut self, session: String) {
        self.agent_session = Some(session);
    }

    /// The attached pi session. Threads exist only in a single-file review the pi launch
    /// command opened; folder, stdin and last-message reviews never have one.
    pub(super) fn attached_session(&self) -> Option<&str> {
        let single_file = self.tree.is_none()
            && !self.open.source.transient
            && matches!(self.open.source.provenance, Provenance::File { .. });
        self.agent_session.as_deref().filter(|_| single_file)
    }

    /// Ctrl-R in the comment box: save the comment as a thread. Unattached, the box stays
    /// open so the text is not lost.
    pub(super) fn save_compose_as_thread(&mut self) -> Result<()> {
        if self.attached_session().is_none() {
            self.status = Some(NOT_ATTACHED.into());
            return Ok(());
        }
        let body = self.compose.value().trim().to_owned();
        self.mode = Mode::Browse;
        if !body.is_empty()
            && let Some(pending) = self.pending.take()
        {
            let rendered = self.open.layout.rendered_in_range(&self.open.doc.source, &pending.range);
            self.open.store.add_thread(&self.open.doc, pending.range, rendered, body)?;
            self.mark_unsent();
            self.sync_tree_counts();
            self.status = Some("thread started".into());
        }
        self.clear_selection();
        Ok(())
    }

    /// Ctrl-R on the rail: make the selected comment a thread, or retry its failed turn.
    pub(super) fn thread_key_on_rail(&mut self) -> Result<()> {
        let Some(id) = self.rail_selected_id() else { return Ok(()) };
        self.retry_thread(&id)
    }

    /// Ctrl-R on annotation `id`, from the rail or the thread panel.
    pub(super) fn retry_thread(&mut self, id: &str) -> Result<()> {
        if self.attached_session().is_none() {
            self.status = Some(NOT_ATTACHED.into());
            return Ok(());
        }
        let status = match self.open.store.thread_key(id)? {
            ThreadKey::Started => {
                self.mark_unsent();
                "thread started"
            }
            ThreadKey::Retried => "retrying",
            ThreadKey::AlreadyThread => "already a thread",
            ThreadKey::Missing => "that comment was removed elsewhere",
        };
        self.status = Some(status.into());
        Ok(())
    }

    /// Ctrl-O on the rail: resolve the selected thread, or unresolve it.
    pub(super) fn resolve_key_on_rail(&mut self) -> Result<()> {
        let Some(id) = self.rail_selected_id() else { return Ok(()) };
        self.toggle_resolved(&id)
    }

    /// Ctrl-O on annotation `id`, from the rail or the thread panel. Needs no agent session:
    /// resolving changes only how the thread is shown and sent.
    pub(super) fn toggle_resolved(&mut self, id: &str) -> Result<()> {
        let thread = self.open.store.threads()?.into_iter().find(|t| t.annotation.id == id).map(|t| t.thread);
        let status = match thread {
            None => "not a thread",
            Some(t) if t.state == ThreadState::Historical => "read-only: this thread was already sent",
            Some(t) => {
                self.open.store.set_resolved(id, !t.resolved)?;
                if t.resolved { "thread unresolved" } else { "thread resolved" }
            }
        };
        self.status = Some(status.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
