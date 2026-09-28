//! A thread: a comment an agent answers, with the conversation carried in its replies.
//!
//! Thread data rides in `Annotation.other` under [`THREAD_KEY`], so a record with threads
//! stays a valid Workspaces annotation list and a client that knows nothing about threads
//! sees a comment with replies. Its absence means a regular comment.
//!
//! The first user message is the annotation's `body`; later messages are `replies` whose
//! `author` is [`USER`] or [`AGENT`], in the order they were sent. A reply of the person's
//! marked [`FOR_KEY`] = [`FOR_MAIN`] is a note for the main agent: the fork never answers it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::annotation::Annotation;

/// The `Annotation.other` key holding a [`Thread`].
pub const THREAD_KEY: &str = "plannotator_tui_thread";
/// `Reply.author` of a message the person wrote.
pub const USER: &str = "user";
/// `Reply.author` of a message the agent wrote.
pub const AGENT: &str = "agent";
/// The `Reply.other` key naming who a message is for, when it is not the thread's fork.
pub const FOR_KEY: &str = "plannotator_tui_for";
/// [`FOR_KEY`]'s value on a note for the main agent.
pub const FOR_MAIN: &str = "main";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub state: ThreadState,
    pub turn: Turn,
    /// Last user message the fork has answered: the annotation's own id stands for its
    /// body, otherwise a reply id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_through: Option<String>,
    /// `None` until the first turn; removed on archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<Fork>,
    /// The person marked it settled. Changes how it is shown and sent, nothing else: a
    /// resolved thread still takes replies and turns. Absent on the wire when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Live,
    /// Archived or restored from the archive: a transcript. Never gets a turn.
    Historical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Turn {
    Idle,
    Running,
    /// `retryable` is false when resending cannot help, as with a context overflow.
    Failed {
        detail: String,
        retryable: bool,
    },
    Interrupted,
}

/// The pi session answering a thread, and what it was copied from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fork {
    pub session_path: PathBuf,
    /// Main's pi session id.
    pub origin_session: String,
    /// Main's session file and the last entry id copied from it.
    pub fork_point: String,
    /// `provider/id`.
    pub model: String,
    pub thinking: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    User,
    Agent,
    /// The person, writing to the main agent rather than the thread's fork.
    Note,
}

impl Author {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => USER,
            Self::Agent => AGENT,
            Self::Note => "note",
        }
    }
}

/// One message of a thread, borrowed from the annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message<'a> {
    /// The annotation id for the first message, else the reply id.
    pub id: &'a str,
    pub author: Author,
    pub body: &'a str,
}

impl Default for Thread {
    fn default() -> Self {
        Self {
            state: ThreadState::Live,
            turn: Turn::Idle,
            answered_through: None,
            fork: None,
            resolved: false,
        }
    }
}

impl Thread {
    /// The thread carried by `annotation`; `Ok(None)` for a regular comment.
    pub fn of(annotation: &Annotation) -> Result<Option<Self>, serde_json::Error> {
        annotation.other.get(THREAD_KEY).map(Self::deserialize).transpose()
    }

    /// Write this thread onto `annotation`, replacing any it had.
    pub fn store_on(&self, annotation: &mut Annotation) -> Result<(), serde_json::Error> {
        annotation.other.insert(THREAD_KEY.to_owned(), serde_json::to_value(self)?);
        Ok(())
    }

    /// User messages after `answered_through`, oldest first: what the next turn answers.
    pub fn unanswered<'a>(&self, annotation: &'a Annotation) -> Vec<Message<'a>> {
        let all = messages(annotation);
        let start = self
            .answered_through
            .as_deref()
            .and_then(|through| all.iter().position(|m| m.id == through))
            .map_or(0, |i| i + 1);
        all.into_iter().skip(start).filter(|m| m.author == Author::User).collect()
    }

    /// A live, idle thread with a user message the fork has not answered.
    pub fn needs_turn(&self, annotation: &Annotation) -> bool {
        self.state == ThreadState::Live && self.turn == Turn::Idle && !self.unanswered(annotation).is_empty()
    }
}

/// Every message of `annotation` in send order: its body, then its replies. A reply not
/// written by the agent counts as the person's, and is a note when marked for main.
pub fn messages(annotation: &Annotation) -> Vec<Message<'_>> {
    let first = Message { id: &annotation.id, author: Author::User, body: &annotation.body };
    let replies = annotation.replies.iter().map(|reply| Message {
        id: &reply.id,
        author: if reply.author.as_deref() == Some(AGENT) {
            Author::Agent
        } else if reply.other.get(FOR_KEY).and_then(|v| v.as_str()) == Some(FOR_MAIN) {
            Author::Note
        } else {
            Author::User
        },
        body: &reply.body,
    });
    std::iter::once(first).chain(replies).collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::anchor::Anchor;
    use crate::annotation::{Reply, State};

    fn annotation(body: &str) -> Annotation {
        Annotation {
            id: "a1".into(),
            document_id: String::new(),
            anchor: Anchor::default(),
            body: body.into(),
            author: None,
            author_name: None,
            state: State::Open,
            attachments: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
            replies: Vec::new(),
            other: BTreeMap::new(),
        }
    }

    fn reply(id: &str, author: &str, body: &str) -> Reply {
        Reply {
            id: id.into(),
            annotation_id: "a1".into(),
            body: body.into(),
            author: Some(author.into()),
            author_name: None,
            created_at: String::new(),
            updated_at: String::new(),
            other: BTreeMap::new(),
        }
    }

    #[test]
    fn an_annotation_without_thread_data_is_a_regular_comment() {
        assert_eq!(Thread::of(&annotation("hi")).expect("readable"), None);
    }

    #[test]
    fn a_resolved_thread_says_so_on_the_wire_and_a_thread_without_the_field_is_unresolved() {
        let mut a = annotation("why?");
        Thread { resolved: true, ..Thread::default() }.store_on(&mut a).expect("stored");
        let json = serde_json::to_value(&a).expect("serializes");
        assert_eq!(json[THREAD_KEY]["resolved"], true);
        let older: Thread =
            serde_json::from_value(serde_json::json!({"state": "live", "turn": {"status": "idle"}}))
                .expect("reads");
        assert!(!older.resolved);
    }

    #[test]
    fn a_thread_rides_in_the_annotation_under_its_key_in_the_wire_shape() {
        let mut a = annotation("why?");
        let thread = Thread {
            turn: Turn::Failed { detail: "rate limited (429)".into(), retryable: true },
            answered_through: Some("a1".into()),
            fork: Some(Fork {
                session_path: "/s/fork.jsonl".into(),
                origin_session: "main-id".into(),
                fork_point: "/s/main.jsonl#e9".into(),
                model: "claude-bridge/claude-haiku-4-5".into(),
                thinking: "medium".into(),
            }),
            ..Thread::default()
        };
        thread.store_on(&mut a).expect("stored");
        let json = serde_json::to_value(&a).expect("serializes");
        assert_eq!(
            json[THREAD_KEY],
            serde_json::json!({
                "state": "live",
                "turn": {"status": "failed", "detail": "rate limited (429)", "retryable": true},
                "answered_through": "a1",
                "fork": {
                    "session_path": "/s/fork.jsonl",
                    "origin_session": "main-id",
                    "fork_point": "/s/main.jsonl#e9",
                    "model": "claude-bridge/claude-haiku-4-5",
                    "thinking": "medium"
                }
            })
        );
        let back: Annotation = serde_json::from_value(json).expect("reads back");
        assert_eq!(Thread::of(&back).expect("readable"), Some(thread));
    }

    #[test]
    fn messages_are_the_body_then_the_replies_in_send_order() {
        let mut a = annotation("first");
        a.replies = vec![reply("r1", USER, "queued"), reply("r2", AGENT, "answer")];
        let got: Vec<(&str, Author, &str)> = messages(&a).iter().map(|m| (m.id, m.author, m.body)).collect();
        assert_eq!(
            got,
            [("a1", Author::User, "first"), ("r1", Author::User, "queued"), ("r2", Author::Agent, "answer")]
        );
    }

    fn note(id: &str, body: &str) -> Reply {
        let mut r = reply(id, USER, body);
        r.other.insert(FOR_KEY.into(), FOR_MAIN.into());
        r
    }

    #[test]
    fn a_reply_for_the_main_agent_is_a_note_and_keeps_its_human_author_on_the_wire() {
        let mut a = annotation("why?");
        a.replies = vec![note("n1", "fix it later")];
        let got: Vec<Author> = messages(&a).iter().map(|m| m.author).collect();
        assert_eq!(got, [Author::User, Author::Note]);
        let json = serde_json::to_value(&a.replies[0]).expect("serializes");
        assert_eq!((json["author"].as_str(), json[FOR_KEY].as_str()), (Some(USER), Some(FOR_MAIN)));
    }

    #[test]
    fn a_note_never_starts_a_turn_and_is_skipped_among_replies() {
        let mut a = annotation("why?");
        a.replies = vec![reply("r1", AGENT, "because"), note("n1", "for main")];
        let answered = Thread { answered_through: Some("a1".into()), ..Thread::default() };
        assert!(!answered.needs_turn(&a), "a note alone asks the fork nothing");
        a.replies.push(reply("r2", USER, "and?"));
        let pending: Vec<&str> = answered.unanswered(&a).iter().map(|m| m.id).collect();
        assert_eq!(pending, ["r2"]);
    }

    #[test]
    fn a_new_thread_needs_its_first_turn() {
        let a = annotation("why?");
        assert!(Thread::default().needs_turn(&a));
    }

    #[test]
    fn a_thread_needs_a_turn_only_while_live_idle_and_owed_an_answer() {
        let mut a = annotation("why?");
        a.replies = vec![reply("r1", AGENT, "because")];
        let answered = Thread { answered_through: Some("a1".into()), ..Thread::default() };
        assert!(!answered.needs_turn(&a), "everything answered");

        a.replies.push(reply("r2", USER, "and then?"));
        assert!(answered.needs_turn(&a), "a later user message is owed an answer");
        assert!(!Thread { turn: Turn::Running, ..answered.clone() }.needs_turn(&a), "one turn at a time");
        assert!(!Thread { turn: Turn::Interrupted, ..answered.clone() }.needs_turn(&a), "waits for retry");
        assert!(
            !Thread { state: ThreadState::Historical, ..answered }.needs_turn(&a),
            "historical never runs"
        );
    }

    #[test]
    fn unanswered_is_every_user_message_after_the_last_answered_one() {
        let mut a = annotation("q1");
        a.replies =
            vec![reply("r1", USER, "q2"), reply("r2", AGENT, "late answer to q1"), reply("r3", USER, "q3")];
        let thread = Thread { answered_through: Some("a1".into()), ..Thread::default() };
        let ids: Vec<&str> = thread.unanswered(&a).iter().map(|m| m.id).collect();
        assert_eq!(ids, ["r1", "r3"]);
    }
}
