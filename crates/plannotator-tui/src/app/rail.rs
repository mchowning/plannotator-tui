//! The rail: comments and threads beside the document. Detached threads come first, in a
//! group of their own, then every placed annotation in source order.

use std::ops::Range;

use plannotator_tui_schema::thread::{Author, messages};
use plannotator_tui_schema::{Annotation, Thread, Turn, is_unread};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::draw::{UNREAD, accent};
use super::{App, Focus, glyph, label};
use crate::wrap::wrap_line;

/// One box in the rail.
#[derive(Debug)]
pub(super) struct RailEntry<'a> {
    pub(super) annotation: &'a Annotation,
    /// Where it sits in the source; `None` for a detached thread.
    pub(super) range: Option<&'a Range<usize>>,
    pub(super) thread: Option<Thread>,
}

/// Rows of a thread's latest message shown in its box; the panel has the rest.
const PREVIEW_ROWS: usize = 4;

/// The line under a thread's messages while a turn runs or after one ended badly. Never a
/// message, never sent.
pub(super) fn turn_status(turn: &Turn) -> Option<(String, Style)> {
    match turn {
        Turn::Idle => None,
        Turn::Running => Some(("working…".into(), Style::new().fg(Color::Cyan).italic())),
        Turn::Failed { detail, retryable: true } => {
            Some((format!("turn failed: {detail}"), Style::new().fg(Color::Red)))
        }
        Turn::Failed { detail, retryable: false } => {
            Some((format!("turn failed: {detail} · retrying won't help"), Style::new().fg(Color::Red)))
        }
        Turn::Interrupted => Some(("turn interrupted".into(), Style::new().fg(Color::Yellow))),
    }
}

impl App {
    /// Every rail box, in display order. A detached regular comment is not in the rail.
    pub(super) fn rail(&self) -> Vec<RailEntry<'_>> {
        let detached = self
            .open
            .store
            .threads()
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.detached)
            .map(|t| RailEntry { annotation: t.annotation, range: None, thread: Some(t.thread) });
        let placed = self.open.store.placed().into_iter().map(|p| RailEntry {
            annotation: p.annotation,
            range: Some(p.range),
            thread: Thread::of(p.annotation).ok().flatten(),
        });
        detached.chain(placed).collect()
    }

    pub(super) fn rail_selected_id(&self) -> Option<String> {
        self.rail().get(self.rail_cursor).map(|e| e.annotation.id.clone())
    }

    /// Also hands focus back to the document once the rail has nothing left to select.
    pub(super) fn clamp_rail_cursor(&mut self) {
        let len = self.rail().len();
        self.rail_cursor = self.rail_cursor.min(len.saturating_sub(1));
        if len == 0 && self.focus == Focus::Rail {
            self.focus = Focus::Document;
        }
    }

    /// The placed card nearest the document selection, measured in rows: one on the
    /// selection first, then the closest above or below, the one below on a tie.
    pub(super) fn nearest_rail_card(&self) -> Option<usize> {
        let here = self.selection_rows();
        self.rail()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((index, self.source_rows(entry.range?)?)))
            .min_by_key(|(_, rows)| {
                if rows.start >= here.end {
                    (rows.start - here.end, 0)
                } else if rows.end <= here.start {
                    (here.start - rows.end, 1)
                } else {
                    (0, 0)
                }
            })
            .map(|(index, _)| index)
    }

    /// Document rows of a `v`/`V` selection, else of the selected block or part.
    fn selection_rows(&self) -> Range<usize> {
        match self.selection.filter(|s| !s.is_empty()) {
            Some(selection) => {
                let (start, end) = selection.ordered();
                start.0..end.0 + 1
            }
            None => self.selected_rows(),
        }
    }

    /// Document rows showing any of a source range, across every block it touches.
    fn source_rows(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let layout = &self.open.layout;
        self.open
            .doc
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.range.start < range.end && range.start < block.range.end)
            .map(|(index, _)| layout.rows_in_range(index, range))
            .filter(|rows| !rows.is_empty())
            .reduce(|a, b| a.start.min(b.start)..a.end.max(b.end))
    }

    /// A box shown as one dim row of `body`, with room kept for the `…` when it is cut short.
    fn collapsed_line(body: &str, width: usize) -> Option<Line<'static>> {
        let body = body.trim();
        let first_line = body.lines().next().unwrap_or("");
        let rows = wrap_line(&Line::from(first_line.to_owned()), &[], width.saturating_sub(1));
        let cut = rows.len() > 1 || first_line.len() < body.len();
        rows.into_iter().next().map(|first| {
            let mut row = first.line.style(Style::new().dim());
            if cut {
                row.push_span(Span::raw("…").dim());
            }
            row
        })
    }

    /// A comment not `selected` collapses like a resolved thread.
    fn bubble_lines(entry: &RailEntry<'_>, width: usize, selected: bool) -> Vec<Line<'static>> {
        let wrapped = |text: &str, style: Style| -> Vec<Line<'static>> {
            wrap_line(&Line::from(text.to_owned()), &[], width)
                .into_iter()
                .map(|r| r.line.style(style))
                .collect()
        };
        let Some(thread) = &entry.thread else {
            let kind = entry.annotation.anchor.kind();
            return if entry.annotation.body.is_empty() {
                wrapped(label(kind), Style::new().dim().italic())
            } else if !selected {
                Self::collapsed_line(&entry.annotation.body, width).into_iter().collect()
            } else {
                wrapped(&entry.annotation.body, Style::new())
            };
        };
        let mut lines = Vec::new();
        if entry.range.is_none() {
            lines.extend(wrapped(entry.annotation.anchor.rendered(), Style::new().dim().crossed_out()));
        }
        if thread.resolved {
            // Collapsed to one row of the first message.
            lines.extend(Self::collapsed_line(&entry.annotation.body, width));
        } else if let Some(latest) = messages(entry.annotation).last() {
            let who = match latest.author {
                Author::User => "you",
                Author::Agent => "agent",
                Author::Note => "note",
            };
            let text = format!("{who}: {}", latest.body);
            let mut rows: Vec<Line<'static>> =
                text.split('\n').flat_map(|line| wrapped(line, Style::new())).collect();
            if rows.len() > PREVIEW_ROWS {
                rows.truncate(PREVIEW_ROWS);
                rows.push(Line::from(Span::raw("…").dim()));
            }
            lines.extend(rows);
        }
        if let Some((text, style)) = turn_status(&thread.turn) {
            lines.extend(wrapped(&text, style));
        }
        lines
    }

    pub(super) fn draw_rail(&mut self, frame: &mut Frame, rail: Rect) {
        let view_end = self.scroll + usize::from(rail.height);
        let rail_focused = self.focus == Focus::Rail;
        let part = self.selected_part().map(|(_, range)| range);
        let mut next_y = rail.y;
        let entries = self.rail();
        let mut bubbles = Vec::new();
        if entries.iter().any(|e| e.range.is_none()) {
            let heading = Line::from(Span::raw(" detached").bold().fg(Color::DarkGray));
            frame.render_widget(Paragraph::new(heading), Rect { height: 1, ..rail });
            next_y += 1;
        }
        for (index, entry) in entries.iter().enumerate() {
            let block = entry.range.and_then(|range| self.open.doc.block_containing(range.start));
            let y = match (entry.range, block) {
                (None, _) => next_y,
                (Some(range), Some(block)) => {
                    let Some(rendered) = self.open.layout.blocks.get(block) else { continue };
                    let anchor_row =
                        self.open.layout.first_row_in_range(block, range).unwrap_or(rendered.first_row);
                    if anchor_row + 1 < self.scroll.saturating_sub(2) || anchor_row >= view_end {
                        continue;
                    }
                    (rail.y + anchor_row.saturating_sub(self.scroll) as u16).max(next_y)
                }
                (Some(_), None) => continue,
            };
            if y >= rail.bottom() {
                break;
            }
            let kind = entry.annotation.anchor.kind();
            let highlighted = match (&part, entry.range) {
                _ if rail_focused => index == self.rail_cursor,
                (Some(part), Some(range)) => part.contains(&range.start),
                _ => block == Some(self.selected),
            };
            let lines = Self::bubble_lines(entry, usize::from(rail.width.saturating_sub(4)), highlighted);
            let height = (lines.len() as u16 + 2).min(rail.bottom().saturating_sub(y));
            if height < 3 {
                break;
            }
            let unread = is_unread(entry.annotation);
            let border = match (highlighted, unread) {
                (true, _) => Style::new().fg(accent(kind)),
                (false, true) => Style::new().fg(UNREAD),
                (false, false) => Style::new().fg(Color::DarkGray),
            };
            let border = if rail_focused && index == self.rail_cursor { border.bold() } else { border };
            let resolved = entry.thread.as_ref().is_some_and(|t| t.resolved);
            let thread = match &entry.thread {
                Some(_) if resolved => " · thread · resolved",
                Some(_) => " · thread",
                None => "",
            };
            let sent = if self.is_file_review() && !self.open.store.is_pending(entry.annotation) {
                " · sent"
            } else {
                ""
            };
            let title = Span::styled(
                format!(" {}{thread}{sent} ", glyph(kind)),
                match (unread, resolved || (entry.thread.is_none() && !highlighted)) {
                    (true, _) => Style::new().fg(UNREAD),
                    (false, true) => Style::new().fg(Color::DarkGray),
                    (false, false) => Style::new().fg(accent(kind)),
                },
            );
            let bubble = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border)
                .title(title);
            let rect = Rect { x: rail.x, y, width: rail.width, height };
            let inner = bubble.inner(rect);
            frame.render_widget(bubble, rect);
            let text_area = Rect { x: inner.x + 1, width: inner.width.saturating_sub(1), ..inner };
            frame.render_widget(Paragraph::new(lines), text_area);
            bubbles.push((rect, entry.annotation.id.clone()));
            next_y = y + height;
        }
        self.geometry.bubbles = bubbles;
    }
}

#[cfg(test)]
mod tests;
