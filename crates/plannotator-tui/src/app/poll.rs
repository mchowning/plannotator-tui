//! Picking up changes other processes make to the record, such as a thread reply the pi
//! runner wrote through the `thread` CLI.

use std::time::{Duration, Instant, SystemTime};

use super::App;

/// Re-read at least this often, for changes the file stats miss (same length, and an
/// mtime that did not move within the filesystem's resolution).
const REREAD_EVERY: Duration = Duration::from_secs(1);

/// What the last poll saw: the record's (mtime, length), and when it was last read.
#[derive(Debug, Default)]
pub(super) struct RecordPoll {
    stat: Option<(Option<SystemTime>, u64)>,
    read_at: Option<Instant>,
}

impl App {
    /// Called every event-loop tick. Stats the record, and re-reads it when the stats
    /// moved or a second has passed. True when the record changed and the screen needs a
    /// redraw.
    pub(crate) fn poll_record(&mut self, now: Instant) -> bool {
        let Some(path) = self.open.store.record_path() else { return false };
        let stat = std::fs::metadata(path).ok().map(|m| (m.modified().ok(), m.len()));
        let due = self.record_poll.read_at.is_none_or(|at| now.duration_since(at) >= REREAD_EVERY);
        if stat == self.record_poll.stat && !due {
            return false;
        }
        self.record_poll = RecordPoll { stat, read_at: Some(now) };
        match self.open.store.refresh(&self.open.doc) {
            Ok(false) => false,
            Ok(true) => {
                self.rail_cursor = self.rail_cursor.min(self.open.store.placed().len().saturating_sub(1));
                self.refresh_review_counts();
                self.derive_send_state();
                self.sync_tree_counts();
                true
            }
            Err(error) => {
                self.status = Some(format!("reading annotations: {error:#}"));
                true
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]
mod tests {
    use std::time::{Duration, Instant};

    use plannotator_tui_schema::{Kind, Provenance};

    use crate::app::review_test_support::file_app;
    use crate::doc::Document;
    use crate::store::{Location, Store};

    /// A second writer on the same record, as the `thread` CLI is.
    fn other_writer(app: &crate::app::App) -> (Store, Document) {
        let Provenance::File { path } = &app.open.source.provenance else { unreachable!("a file review") };
        let doc = Document::parse(std::fs::read_to_string(path).expect("doc"));
        let store = Store::load(&Location::for_file(&app.data_dir, &app.project, path), &doc).expect("load");
        (store, doc)
    }

    #[test]
    fn a_reply_another_process_writes_shows_on_the_next_poll() {
        let (_root, mut app, _) = file_app("poll-reply");
        app.attach_agent_session("pi".into());
        app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
        let id = app.open.store.placed()[0].annotation.id.clone();
        app.open.store.thread_key(&id).expect("thread");
        let start = Instant::now();
        assert!(!app.poll_record(start), "nothing changed yet");

        let (mut cli, _) = other_writer(&app);
        cli.add_agent_reply(&id, &id, "Because two comes after one.".into()).expect("reply");

        assert!(app.poll_record(start + Duration::from_millis(250)), "the change is picked up");
        let threads = app.open.store.threads().expect("threads");
        let messages = plannotator_tui_schema::thread::messages(threads[0].annotation);
        assert_eq!(messages.last().map(|m| m.body), Some("Because two comes after one."));
        assert_eq!(app.open.store.placed().len(), 1, "the thread is still placed");
    }

    #[test]
    fn an_annotation_another_process_adds_is_placed_against_the_document() {
        let (_root, mut app, _) = file_app("poll-add");
        let start = Instant::now();
        app.poll_record(start);
        let (mut cli, doc) = other_writer(&app);
        let range = doc.source.find("three").map(|i| i..i + 5).expect("quote");
        cli.add(&doc, range, "three".into(), Kind::Comment, "added elsewhere".into()).expect("add");

        assert!(app.poll_record(start + Duration::from_millis(250)));
        assert_eq!(app.open.store.placed().len(), 1);
    }

    #[test]
    fn a_change_the_file_stats_miss_is_still_read_within_a_second() {
        let (_root, mut app, _) = file_app("poll-second");
        app.add_quote_annotation("two", Kind::Comment, "Why two?".into()).expect("comment");
        let start = Instant::now();
        app.poll_record(start);
        let record = std::fs::read_to_string(app.open.store.record_path().expect("path")).expect("record");
        // Same length, and the mtime restored: a stat check alone sees nothing.
        let edited = record.replace("Why two?", "Why TWO?");
        let path = app.open.store.record_path().expect("path").to_path_buf();
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).expect("mtime");
        std::fs::write(&path, edited).expect("write");
        std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(mtime)).expect("mtime");

        assert!(!app.poll_record(start + Duration::from_millis(250)), "stats unchanged, not yet due");
        assert!(app.poll_record(start + Duration::from_millis(1_100)), "read once the second is up");
        assert_eq!(app.open.store.placed()[0].annotation.body, "Why TWO?");
    }
}
