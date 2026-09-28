//! The `thread` CLI against a real record in a private data dir: the contract the pi thread
//! runner depends on.

#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

struct Fixture {
    root: PathBuf,
    doc: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "plannotator-tui-thread-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs")).expect("dirs");
        let doc = root.join("docs/plan.md");
        std::fs::write(&doc, "# Plan\n\nShip the login page by Friday.\n").expect("doc");
        Self { data: root.join("data"), root, doc }
    }

    fn bin(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_plannotator-tui"));
        command.env("PLANNOTATOR_DATA_DIR", &self.data);
        command
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = self
            .bin()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawns");
        child.stdin.take().expect("stdin").write_all(stdin.as_bytes()).expect("stdin written");
        child.wait_with_output().expect("runs")
    }

    fn thread(&self, args: &[&str], stdin: &str) -> Output {
        let doc = self.doc.display().to_string();
        let mut full = vec!["thread", args[0], doc.as_str()];
        full.extend_from_slice(&args[1..]);
        self.run(&full, stdin)
    }

    fn record_path(&self) -> PathBuf {
        let folder = self.doc.parent().expect("folder");
        let project = plannotator_tui_schema::project_name(None, folder);
        plannotator_tui_schema::annotations_dir(&self.data, &project, &self.doc.to_string_lossy())
            .join("annotations.json")
    }

    fn record(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.record_path()).expect("record")).expect("json")
    }

    fn write_record(&self, record: &Value) {
        std::fs::write(self.record_path(), serde_json::to_string_pretty(record).expect("json"))
            .expect("write");
    }

    /// One comment through the UI's store path, then `threads` copies of it made into live
    /// threads with ids `t0`, `t1`, ….
    fn seed_threads(&self, threads: usize, state: &str) {
        let annotate = self.run(&["--annotate", &self.doc.display().to_string(), "login page", "why?"], "");
        assert!(annotate.status.success(), "{}", String::from_utf8_lossy(&annotate.stderr));
        let mut record = self.record();
        let template = record["annotations"][0].clone();
        let seeded: Vec<Value> = (0..threads)
            .map(|i| {
                let mut a = template.clone();
                a["id"] = json!(format!("t{i}"));
                a["plannotator_tui_thread"] = json!({"state": state, "turn": {"status": "idle"}});
                a
            })
            .collect();
        record["annotations"] = Value::Array(seeded);
        self.write_record(&record);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stdout_json(output: &Output) -> Value {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("json on stdout")
}

#[test]
fn the_review_store_and_the_thread_cli_writing_at_once_lose_nothing() {
    const WRITES: usize = 200;
    let f = Fixture::new("concurrent");
    f.seed_threads(WRITES, "live");
    let doc = f.doc.display().to_string();
    std::thread::scope(|scope| {
        let review = scope.spawn(|| {
            for i in 0..WRITES {
                let out = f.run(&["--annotate", &doc, "Friday", &format!("ui-{i}")], "");
                assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            }
        });
        for i in 0..WRITES {
            let id = format!("t{i}");
            let out = f.thread(&["reply", &id, "--through", &id], &format!("agent-{i}"));
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
        review.join().expect("review writer");
    });
    let record = f.record();
    let annotations = record["annotations"].as_array().expect("annotations");
    let review_writes =
        annotations.iter().filter(|a| a["body"].as_str().is_some_and(|b| b.starts_with("ui-"))).count();
    let answered = (0..WRITES)
        .filter(|i| {
            annotations.iter().any(|a| {
                a["id"] == json!(format!("t{i}")) && a["replies"][0]["body"] == json!(format!("agent-{i}"))
            })
        })
        .count();
    assert_eq!((review_writes, answered), (WRITES, WRITES), "every write from both processes survives");
}

#[test]
fn a_reply_answers_through_a_message_ends_the_turn_and_lists_in_order() {
    let f = Fixture::new("reply");
    f.seed_threads(1, "live");
    stdout_json(&f.thread(&["turn", "t0", "running"], ""));
    assert_eq!(stdout_json(&f.thread(&["list"], ""))["threads"][0]["thread"]["turn"]["status"], "running");

    let reply = stdout_json(&f.thread(&["reply", "t0", "--through", "t0"], "Because Friday is the demo.\n"));
    let listed = stdout_json(&f.thread(&["list"], ""));
    let thread = &listed["threads"][0];
    assert_eq!(thread["id"], "t0");
    assert_eq!(thread["quote"], "login page");
    assert_eq!(thread["detached"], false);
    assert_eq!(thread["thread"]["turn"], json!({"status": "idle"}));
    assert_eq!(thread["thread"]["answered_through"], "t0");
    assert_eq!(
        thread["messages"],
        json!([
            {"id": "t0", "author": "user", "body": "why?"},
            {"id": reply["id"], "author": "agent", "body": "Because Friday is the demo."}
        ])
    );
    let again = f.thread(&["reply", "t0", "--through", "t0"], "twice");
    assert!(!again.status.success(), "a message is answered once");
}

#[test]
fn fork_and_failed_turns_are_recorded_on_the_thread() {
    let f = Fixture::new("fork");
    f.seed_threads(1, "live");
    stdout_json(&f.thread(
        &[
            "fork",
            "t0",
            "--session",
            "/s/fork.jsonl",
            "--origin",
            "main",
            "--fork-point",
            "/s/main.jsonl#e9",
            "--model",
            "claude-bridge/claude-haiku-4-5",
            "--thinking",
            "medium",
        ],
        "",
    ));
    stdout_json(&f.thread(&["turn", "t0", "failed", "--detail", "context overflow", "--no-retry"], ""));
    let thread = &stdout_json(&f.thread(&["list"], ""))["threads"][0]["thread"];
    assert_eq!(thread["fork"]["session_path"], "/s/fork.jsonl");
    assert_eq!(thread["fork"]["model"], "claude-bridge/claude-haiku-4-5");
    assert_eq!(thread["turn"], json!({"status": "failed", "detail": "context overflow", "retryable": false}));
}

#[test]
fn writes_to_a_historical_or_missing_thread_exit_non_zero_and_change_nothing() {
    let f = Fixture::new("historical");
    f.seed_threads(1, "historical");
    let before = std::fs::read(f.record_path()).expect("record");
    for args in [
        vec!["reply", "t0", "--through", "t0"],
        vec!["turn", "t0", "running"],
        vec!["reply", "missing", "--through", "missing"],
        vec!["turn", "missing", "interrupted"],
    ] {
        let out = f.thread(&args, "late answer");
        assert!(!out.status.success(), "{args:?} must fail");
    }
    assert_eq!(std::fs::read(f.record_path()).expect("record"), before, "nothing written");
}

#[test]
fn a_thread_whose_passage_is_gone_lists_as_detached() {
    let f = Fixture::new("detached");
    f.seed_threads(1, "live");
    std::fs::write(&f.doc, "# Plan\n\nEverything changed.\n").expect("edit");
    assert_eq!(stdout_json(&f.thread(&["list"], ""))["threads"][0]["detached"], true);
}

#[test]
fn a_handoff_is_read_and_cleared_only_by_its_id() {
    let f = Fixture::new("handoff");
    f.seed_threads(1, "live");
    assert_eq!(stdout_json(&f.thread(&["handoff"], "")), json!({}));
    let mut record = f.record();
    record["handoff"] = json!({"id": "h1", "owner": "session-a", "body": "# review", "created_at": "2026-09-24T00:00:00.000Z"});
    f.write_record(&record);
    assert_eq!(
        stdout_json(&f.thread(&["handoff"], "")),
        json!({"id": "h1", "owner": "session-a", "body": "# review"})
    );
    assert!(!f.thread(&["handoff-ack", "h2"], "").status.success(), "a different id leaves it pending");
    stdout_json(&f.thread(&["handoff-ack", "h1"], ""));
    assert_eq!(stdout_json(&f.thread(&["handoff"], "")), json!({}));
    assert!(f.record().get("handoff").is_none());
}

#[test]
fn the_record_lock_is_a_sibling_file() {
    let f = Fixture::new("lock");
    f.seed_threads(1, "live");
    stdout_json(&f.thread(&["turn", "t0", "running"], ""));
    let lock: &Path = &f.record_path().with_file_name("annotations.json.lock");
    assert!(lock.is_file());
}

#[test]
fn a_note_for_the_main_agent_lists_with_author_note_so_the_runner_skips_it() {
    let f = Fixture::new("note");
    f.seed_threads(1, "live");
    let mut record = f.record();
    record["annotations"][0]["replies"] = json!([{
        "id": "n1", "annotation_id": "t0", "body": "rename it", "author": "user",
        "created_at": "2026-01-01T00:00:00.000Z", "updated_at": "2026-01-01T00:00:00.000Z",
        "plannotator_tui_for": "main"
    }]);
    f.write_record(&record);
    let listed = stdout_json(&f.thread(&["list"], ""));
    assert_eq!(
        listed["threads"][0]["messages"],
        json!([
            {"id": "t0", "author": "user", "body": "why?"},
            {"id": "n1", "author": "note", "body": "rename it"}
        ])
    );
}
