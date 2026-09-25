//! The thread panel: the passage, the messages in send order, the turn's status line, and
//! a reply input that always has focus. It takes the rail's place at about 42% of the
//! width, so the document stays in view with the passage highlighted.

use anyhow::Result;
use plannotator_tui_schema::thread::{Author, messages};
use plannotator_tui_schema::{Thread, ThreadState};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::compose::{Compose, ComposeAction};
use super::rail::turn_status;
use super::threads::NOT_ATTACHED;
use super::{App, Focus, Mode};
use crate::wrap::wrap_line;

/// The panel's share of the screen width, in percent.
pub(super) const PANEL_PERCENT: u16 = 42;

impl App {
    /// Open the panel on thread `id`, scrolled to its latest message, with its passage
    /// brought into view.
    pub(super) fn open_thread(&mut self, id: String) {
        self.compose = Compose::default();
        self.panel_back = 0;
        let start = self.rail().iter().find(|e| e.annotation.id == id).and_then(|e| e.range.map(|r| r.start));
        if let Some(block) = start.and_then(|s| self.open.doc.block_containing(s)) {
            self.selected = block;
            self.ensure_selected_visible();
        }
        self.mode = Mode::Thread(id);
    }

    fn close_thread(&mut self) {
        self.mode = Mode::Browse;
        self.focus = Focus::Rail;
        self.compose = Compose::default();
    }

    /// The open thread, as the record has it now.
    fn panel_thread(&self) -> Option<(String, Thread)> {
        let Mode::Thread(id) = &self.mode else { return None };
        let entry = self.open.store.threads().ok()?.into_iter().find(|t| &t.annotation.id == id)?;
        Some((id.clone(), entry.thread))
    }

    pub(super) fn thread_panel_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some((id, thread)) = self.panel_thread() else {
            self.close_thread();
            return Ok(());
        };
        let page = usize::from(self.geometry.panel.map_or(10, |p| p.height / 2));
        match key.code {
            KeyCode::PageUp => self.panel_back += page,
            KeyCode::PageDown => self.panel_back = self.panel_back.saturating_sub(page),
            _ => match self.compose.handle_key(key) {
                ComposeAction::Cancel => self.close_thread(),
                ComposeAction::SaveThread => self.retry_thread(&id)?,
                ComposeAction::Save => self.send_panel_reply(&id, &thread)?,
                ComposeAction::Edited => {}
            },
        }
        Ok(())
    }

    fn send_panel_reply(&mut self, id: &str, thread: &Thread) -> Result<()> {
        let body = self.compose.value().trim().to_owned();
        if thread.state == ThreadState::Historical {
            self.status = Some("read-only: this thread was already sent".into());
        } else if self.attached_session().is_none() {
            self.status = Some(NOT_ATTACHED.into());
        } else if !body.is_empty() {
            self.open.store.add_user_reply(id, body)?;
            self.compose = Compose::default();
            self.panel_back = 0;
            self.mark_unsent();
        }
        Ok(())
    }

    pub(super) fn thread_panel_mouse(&mut self, mouse: MouseEvent) {
        let inside = self.geometry.panel.is_some_and(|p| {
            mouse.column >= p.x && mouse.column < p.right() && mouse.row >= p.y && mouse.row < p.bottom()
        });
        match mouse.kind {
            MouseEventKind::ScrollUp if inside => self.panel_back += 3,
            MouseEventKind::ScrollDown if inside => self.panel_back = self.panel_back.saturating_sub(3),
            MouseEventKind::ScrollUp => self.scroll_by(-3),
            MouseEventKind::ScrollDown => self.scroll_by(3),
            _ => {}
        }
    }

    /// The transcript as display lines at `width`: passage, then each message under its
    /// author, then the status line.
    fn panel_lines(&self, id: &str, thread: &Thread, width: usize) -> Vec<Line<'static>> {
        let wrap = |text: &str, style: Style| -> Vec<Line<'static>> {
            text.split('\n')
                .flat_map(|line| wrap_line(&Line::from(line.to_owned()), &[], width))
                .map(|r| r.line.style(style))
                .collect()
        };
        let Some(annotation) = self.open.store.threads().ok().and_then(|ts| {
            ts.into_iter().find(|t| t.annotation.id == id).map(|t| (t.annotation.clone(), t.detached))
        }) else {
            return Vec::new();
        };
        let (annotation, detached) = annotation;
        let mut lines = Vec::new();
        let quote = annotation.anchor.rendered();
        if detached {
            lines.push(Line::from(Span::raw("(passage since changed)").fg(Color::Yellow)));
            lines.extend(wrap(&format!("> {quote}"), Style::new().dim().crossed_out()));
        } else {
            lines.extend(wrap(&format!("> {quote}"), Style::new().dim()));
        }
        for message in messages(&annotation) {
            lines.push(Line::default());
            let (who, color) = match message.author {
                Author::User => ("you", Color::Yellow),
                Author::Agent => ("agent", Color::Cyan),
            };
            lines.push(Line::from(Span::raw(who).bold().fg(color)));
            lines.extend(wrap(message.body, Style::new()));
        }
        if let Some((text, style)) = turn_status(&thread.turn) {
            lines.push(Line::default());
            lines.extend(wrap(&text, style));
        }
        lines
    }

    pub(super) fn draw_thread_panel(&mut self, frame: &mut Frame, area: Rect) {
        let Some((id, thread)) = self.panel_thread() else {
            self.close_thread();
            return;
        };
        let read_only = thread.state == ThreadState::Historical;
        frame.render_widget(Clear, area);
        // The footer lists the keys; the title only has to say what this is.
        let title = if read_only { " thread · read-only · esc closes " } else { " thread · esc closes " };
        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::Cyan))
            .title(Span::styled(title, Style::new().dim()));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);
        self.geometry.panel = Some(area);

        let text_width = usize::from(inner.width.saturating_sub(2));
        let (input_lines, cursor_row, cursor_col) = self.compose.wrapped(text_width);
        let input_rows = if read_only { 0 } else { input_lines.len().clamp(1, 6) as u16 + 2 };
        let transcript = Rect { height: inner.height.saturating_sub(input_rows), ..inner };

        let lines = self.panel_lines(&id, &thread, text_width);
        let visible = usize::from(transcript.height);
        let max_back = lines.len().saturating_sub(visible);
        self.panel_back = self.panel_back.min(max_back);
        let first = max_back - self.panel_back;
        let shown: Vec<Line<'static>> = lines.into_iter().skip(first).take(visible).collect();
        let text_area = Rect { x: transcript.x + 1, width: transcript.width.saturating_sub(1), ..transcript };
        frame.render_widget(Paragraph::new(shown), text_area);

        if read_only {
            return;
        }
        let input = Rect { y: inner.bottom().saturating_sub(input_rows), height: input_rows, ..inner };
        let boxed = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::Yellow))
            .title(Span::styled(" reply ", Style::new().dim()));
        let input_inner = boxed.inner(input);
        frame.render_widget(boxed, input);
        let rows = usize::from(input_rows.saturating_sub(2)).max(1);
        let scroll = cursor_row.saturating_sub(rows - 1);
        let body: Vec<Line<'static>> =
            input_lines.into_iter().skip(scroll).take(rows).map(Line::from).collect();
        let body_area =
            Rect { x: input_inner.x + 1, width: input_inner.width.saturating_sub(1), ..input_inner };
        frame.render_widget(Paragraph::new(body), body_area);
        let x = (body_area.x + cursor_col as u16).min(body_area.right().saturating_sub(1));
        frame.set_cursor_position((x, body_area.y + (cursor_row - scroll) as u16));
    }
}

#[cfg(test)]
mod tests;
