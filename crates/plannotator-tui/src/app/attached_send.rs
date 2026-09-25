//! Sending an attached review to the pi session that opened it. Quitting is the send
//! confirmation: `y` writes the review into the record as a handoff for that session and
//! archives it; `n` closes and changes nothing; `Esc` stays.

use plannotator_tui_schema::{Provenance, Turn};

use super::feedback::{Feedback, SendScope};
use super::{App, Mode, Open};

impl App {
    /// `q` or `E`: ask before sending. With nothing to send, `q` just closes.
    pub(super) fn ask_to_send_attached(&mut self, quitting: bool) {
        if self.file_feedback(SendScope::Pending).count > 0 {
            self.mode = Mode::ConfirmQuit;
        } else if quitting {
            self.quit = true;
        } else {
            self.status = Some("nothing new to send".into());
        }
    }

    pub(super) fn attached_question(&self) -> String {
        let running =
            self.open.store.threads().unwrap_or_default().iter().any(|t| t.thread.turn == Turn::Running);
        let abort = if running { " · running turns will be aborted" } else { "" };
        format!(" send this review to pi? y send · n keep for later · esc stay{abort}")
    }

    /// Commit the send, then close. A refusal (an earlier review still undelivered, or the
    /// record unwritable) keeps the review open with the reason in the footer.
    pub(super) fn send_attached(&mut self) {
        let Some(owner) = self.attached_session().map(str::to_owned) else { return };
        let Provenance::File { path } = self.open.source.provenance.clone() else { return };
        let name = self.open.source.name.clone();
        let mut rendered = Feedback::default();
        let Open { doc, store, .. } = &mut self.open;
        let sent = store.hand_off(doc, &owner, "pi", |view| {
            rendered.add(Some(path.clone()), &name, doc, view.clone(), SendScope::Pending);
            let ids = rendered.parts.iter().flat_map(|p| p.ids.clone()).collect();
            (rendered.text.clone(), ids)
        });
        match sent {
            Ok(_) => {
                self.archive_submission(&mut rendered);
                self.mark_unsent();
                self.sync_tree_counts();
                self.status = Some(format!("sent {} annotation(s) → pi", rendered.count));
                self.quit = true;
            }
            Err(error) => self.status = Some(format!("not sent: {error:#}")),
        }
    }
}

#[cfg(test)]
mod tests;
