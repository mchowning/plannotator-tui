//! Feedback export: annotations as numbered Markdown a coding agent reads without a schema.
//!
//! `# Annotations on <name>`, then one `## Annotation N (line X)` per annotation in document
//! order: what kind of note it is, the quoted text, and the body as a blockquote. Deleted
//! text is fenced so quoted markdown cannot escape. A thread is its passage and its
//! messages labeled by author; one whose passage is gone comes last, marked as such.

use std::fmt::Write as _;
use std::ops::Range;

use plannotator_tui_schema::thread::{Author, messages};
use plannotator_tui_schema::{Annotation, Kind, Thread};

/// One annotation placed in the document, as the exporter needs it.
pub(crate) struct Entry<'a> {
    pub(crate) annotation: &'a Annotation,
    pub(crate) range: Range<usize>,
    /// 1-based source line span of the annotated range.
    pub(crate) lines: (usize, usize),
}

/// The body with no detached threads; the app always goes through
/// `feedback_with_detached`.
#[cfg(test)]
pub(crate) fn feedback(source: &str, name: &str, entries: &[Entry<'_>]) -> String {
    feedback_with_detached(source, name, entries, &[])
}

/// `feedback`, followed by threads whose passage no longer resolves.
pub(crate) fn feedback_with_detached(
    source: &str,
    name: &str,
    entries: &[Entry<'_>],
    detached: &[&Annotation],
) -> String {
    if entries.is_empty() && detached.is_empty() {
        return "No annotations.".to_owned();
    }
    let mut out = format!("# Annotations on {name}\n\n");
    for (i, entry) in entries.iter().enumerate() {
        let quoted = source.get(entry.range.clone()).unwrap_or("");
        let line_label = match entry.lines {
            (a, b) if a == b => format!("line {a}"),
            (a, b) => format!("lines {a}\u{2013}{b}"),
        };
        let _ = writeln!(out, "## Annotation {} ({line_label})", i + 1);
        if is_thread(entry.annotation) {
            write_thread(&mut out, &single_line(quoted), entry.annotation);
            continue;
        }
        let body = entry.annotation.body.trim();
        match entry.annotation.anchor.kind() {
            Kind::Delete => {
                out.push_str("Remove this:\n");
                out.push_str(&fenced(quoted));
                let _ = writeln!(out, "> {}", if body.is_empty() { "I don't want this." } else { body });
            }
            Kind::LooksGood => {
                let _ = writeln!(out, "Looks good: \"{}\"", single_line(quoted));
                if !body.is_empty() {
                    let _ = writeln!(out, "> {}", quote_lines(body));
                }
            }
            Kind::Comment => {
                let _ = writeln!(out, "Comment on: \"{}\"", single_line(quoted));
                let _ = writeln!(out, "> {}", quote_lines(body));
            }
        }
        for reply in &entry.annotation.replies {
            let who = reply.author.as_deref().unwrap_or("reply");
            let _ = writeln!(out, "- **Reply ({who}):** {}", reply.body.replace('\n', "\n  "));
        }
        out.push('\n');
    }
    for (i, annotation) in detached.iter().enumerate() {
        let _ = writeln!(out, "## Annotation {} (passage since changed)", entries.len() + i + 1);
        write_thread(&mut out, &single_line(annotation.anchor.rendered()), annotation);
    }
    out
}

fn is_thread(annotation: &Annotation) -> bool {
    Thread::of(annotation).ok().flatten().is_some()
}

fn write_thread(out: &mut String, quote: &str, annotation: &Annotation) {
    let _ = writeln!(out, "Thread on: \"{quote}\"");
    for message in messages(annotation) {
        let who = match message.author {
            Author::User => "user",
            Author::Agent => "agent",
        };
        let _ = writeln!(out, "- **{who}:** {}", message.body.trim().replace('\n', "\n  "));
    }
    out.push('\n');
}

/// A fence longer than any backtick run inside the text, so quoted markdown cannot escape.
fn fenced(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{text}\n{fence}\n")
}

fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn quote_lines(text: &str) -> String {
    text.replace('\n', "\n> ")
}

/// 1-based line numbers of the first and last byte of `range`.
pub(crate) fn line_span(source: &str, range: &Range<usize>) -> (usize, usize) {
    let line_at = |offset: usize| source.get(..offset).map_or(1, |s| s.matches('\n').count() + 1);
    (line_at(range.start), line_at(range.end.saturating_sub(1).max(range.start)))
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "tests assert by panicking")]
mod tests {
    use super::*;
    use plannotator_tui_schema::{Anchor, SourceRange, State};

    fn annotation(source: &str, quote: &str, kind: Kind, body: &str) -> (Annotation, Range<usize>) {
        let start = source.find(quote).expect("present");
        let range = start..start + quote.len();
        let source_range = SourceRange { start, end: range.end, version: "v".into() };
        let annotation = Annotation {
            id: "a".into(),
            document_id: String::new(),
            anchor: Anchor::new(quote, source, source_range, kind, None),
            body: body.into(),
            author: None,
            author_name: None,
            state: State::Open,
            attachments: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
            replies: Vec::new(),
            other: std::collections::BTreeMap::default(),
        };
        (annotation, range)
    }

    #[test]
    fn export_matches_the_agent_facing_shape() {
        let source = "# Title\n\nShip the login page by Friday.\n\nDrop the `legacy` path.\n";
        let (comment, r1) = annotation(source, "login page", Kind::Comment, "Which page?\nBe specific.");
        let (delete, r2) = annotation(source, "Drop the `legacy` path.", Kind::Delete, "");
        let entries = [
            Entry { annotation: &comment, lines: line_span(source, &r1), range: r1 },
            Entry { annotation: &delete, lines: line_span(source, &r2), range: r2 },
        ];
        let out = feedback(source, "plan.md", &entries);
        assert_eq!(
            out,
            "# Annotations on plan.md\n\n\
             ## Annotation 1 (line 3)\nComment on: \"login page\"\n> Which page?\n> Be specific.\n\n\
             ## Annotation 2 (line 5)\nRemove this:\n```\nDrop the `legacy` path.\n```\n> I don't want this.\n\n"
        );
    }

    #[test]
    fn fences_grow_past_embedded_backticks() {
        assert!(fenced("has ``` inside").starts_with("````\n"));
    }
}
