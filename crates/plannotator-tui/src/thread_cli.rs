//! `plannotator-tui thread …`: the contract between the pi thread runner and the record.
//!
//! Every command prints JSON; a non-zero exit means nothing was written. The document path
//! is keyed exactly as given (after making it absolute), the same rule the review UI uses,
//! so both name one record.

use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use plannotator_tui_schema::{Fork, Turn, messages};
use serde_json::{Value, json};

use crate::cli::USAGE;
use crate::doc::Document;
use crate::store::{Location, Store};
use crate::workspace_paths;

/// Run one `thread` subcommand; returns the JSON to print.
pub(crate) fn run(args: &[String]) -> Result<String> {
    let sub = args.first().map(String::as_str);
    let doc = args.get(1).context(USAGE)?;
    let (_document, mut store) = open(Path::new(doc))?;
    let id = || args.get(2).map(String::as_str).context(USAGE);
    let flags = Flags(args.get(3..).unwrap_or_default());
    let out = match sub {
        Some("list") => list(&store)?,
        Some("fork") => {
            store.set_fork(
                id()?,
                Fork {
                    session_path: flags.value("--session")?.into(),
                    origin_session: flags.value("--origin")?.to_owned(),
                    fork_point: flags.value("--fork-point")?.to_owned(),
                    model: flags.value("--model")?.to_owned(),
                    thinking: flags.value("--thinking")?.to_owned(),
                },
            )?;
            json!({})
        }
        Some("turn") => {
            let id = id()?;
            let flags = Flags(args.get(4..).unwrap_or_default());
            let turn = match args.get(3).map(String::as_str) {
                Some("running") => Turn::Running,
                Some("interrupted") => Turn::Interrupted,
                Some("failed") => Turn::Failed {
                    detail: flags.optional("--detail")?.unwrap_or("unknown error").to_owned(),
                    retryable: !flags.has("--no-retry"),
                },
                _ => bail!("turn needs running, interrupted or failed\n{USAGE}"),
            };
            store.set_turn(id, turn)?;
            json!({})
        }
        Some("reply") => {
            let id = id()?;
            let through = flags.value("--through")?;
            let mut body = String::new();
            std::io::stdin().read_to_string(&mut body).context("reading the reply from stdin")?;
            let body = body.trim().to_owned();
            if body.is_empty() {
                bail!("empty reply for thread {id}");
            }
            json!({ "id": store.add_agent_reply(id, through, body)? })
        }
        Some("handoff") => match store.handoff()? {
            Some(h) => json!({ "id": h.id, "owner": h.owner, "body": h.body }),
            None => json!({}),
        },
        Some("handoff-ack") => {
            store.ack_handoff(id()?)?;
            json!({})
        }
        _ => bail!("unknown thread command\n{USAGE}"),
    };
    Ok(format!("{out}\n"))
}

/// The record for `path`, resolved against the file as it is now. A missing file resolves
/// nothing, so its threads read as detached.
fn open(path: &Path) -> Result<(Document, Store)> {
    let path = workspace_paths::absolute(path);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    let document = Document::parse(content);
    let folder = path.parent().unwrap_or(Path::new("/"));
    let location =
        Location::for_file(&workspace_paths::data_dir(), &workspace_paths::project_name(folder), &path);
    let store = Store::load(&location, &document)?;
    Ok((document, store))
}

fn list(store: &Store) -> Result<Value> {
    let threads = store
        .threads()?
        .into_iter()
        .map(|entry| {
            let messages: Vec<Value> = messages(entry.annotation)
                .iter()
                .map(|m| json!({ "id": m.id, "author": m.author.as_str(), "body": m.body }))
                .collect();
            json!({
                "id": entry.annotation.id,
                "quote": entry.annotation.anchor.rendered(),
                "detached": entry.detached,
                "thread": entry.thread,
                "messages": messages,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "threads": threads }))
}

/// `--name value` pairs after the positional arguments.
struct Flags<'a>(&'a [String]);

impl Flags<'_> {
    fn optional(&self, name: &str) -> Result<Option<&str>> {
        match self.0.iter().position(|a| a == name) {
            Some(i) => {
                self.0.get(i + 1).map(|v| Some(v.as_str())).with_context(|| format!("{name} needs a value"))
            }
            None => Ok(None),
        }
    }

    fn value(&self, name: &str) -> Result<&str> {
        self.optional(name)?.with_context(|| format!("missing {name}\n{USAGE}"))
    }

    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
}
