//! Block mode inside a table or a list: `j`/`k` step through its parts (table rows, list
//! items), and the part the cursor is on is what gets highlighted and commented on. The
//! part is read off the cursor rather than stored, so every path that moves the cursor
//! keeps it right.

use std::ops::Range;

use super::App;

impl App {
    /// The selected block's part under the cursor: its index and source range.
    pub(super) fn selected_part(&self) -> Option<(usize, Range<usize>)> {
        let layout = &self.open.layout;
        if layout.block_at_row(self.cursor.0) != Some(self.selected) {
            return None;
        }
        let offset = layout.row(self.cursor.0)?.cells.iter().flatten().next().copied()?;
        let rows = self.open.doc.parts(self.selected);
        rows.iter().position(|r| r.contains(&offset)).and_then(|i| Some((i, rows.get(i)?.clone())))
    }

    /// Document rows highlighted as the selection in block mode: the part under the
    /// cursor, else the whole selected block.
    pub(super) fn selected_rows(&self) -> Range<usize> {
        let layout = &self.open.layout;
        if let Some((_, range)) = self.selected_part() {
            return layout.rows_in_range(self.selected, &range);
        }
        layout.blocks.get(self.selected).map_or(0..0, |b| b.first_row..b.first_row + b.rows.len())
    }

    /// Put the cursor on part `index` of the selected block and bring it into view.
    pub(super) fn select_part(&mut self, index: usize) {
        let Some(range) = self.open.doc.parts(self.selected).get(index) else { return };
        self.cursor = (self.open.layout.rows_in_range(self.selected, range).start, 0);
        self.ensure_selected_visible();
    }

    /// Put the cursor on the selected block's first part at or below document row `row`,
    /// else its last part, leaving the view where it is: paging owns the view.
    pub(super) fn select_part_from(&mut self, row: usize) {
        let layout = &self.open.layout;
        let starts: Vec<usize> = self
            .open
            .doc
            .parts(self.selected)
            .iter()
            .map(|r| layout.rows_in_range(self.selected, r).start)
            .collect();
        if let Some(&start) = starts.iter().find(|&&s| s >= row).or(starts.last()) {
            self.cursor = (start, 0);
        }
    }

    /// `j` (+1) or `k` (-1): the next part while there is one, else the next block,
    /// entering a block with parts from below on its last part.
    pub(super) fn step(&mut self, delta: isize) {
        let rows = self.open.doc.parts(self.selected).len();
        if let Some((index, _)) = self.selected_part()
            && let Some(next) = index.checked_add_signed(delta).filter(|&n| n < rows)
        {
            self.clear_selection();
            self.roam = false;
            self.select_part(next);
            return;
        }
        let block =
            self.selected.saturating_add_signed(delta).min(self.open.doc.blocks.len().saturating_sub(1));
        if block == self.selected && self.selected_part().is_some() {
            // At the document's edge on a part: stay on it.
            return;
        }
        self.select_block(block);
        if delta < 0
            && let Some(last) = self.open.doc.parts(self.selected).len().checked_sub(1)
        {
            self.select_part(last);
        }
    }

    /// Select `offset`'s block with the cursor on its row, as the rail does for a note.
    pub(super) fn select_offset(&mut self, offset: usize) {
        let Some(block) = self.open.doc.block_containing(offset) else { return };
        self.selected = block;
        if let Some(row) = self.open.layout.first_row_in_range(block, &(offset..offset + 1)) {
            self.cursor = (row, 0);
        }
        self.ensure_selected_visible();
    }
}

#[cfg(test)]
mod tests;
