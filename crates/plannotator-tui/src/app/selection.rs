//! A mouse selection in document coordinates, and its conversion to a source range.

use std::ops::Range;

/// Anchor and head are (row, column) in document coordinates.
#[derive(Debug, Clone, Copy)]
pub(super) struct Selection {
    anchor: (usize, usize),
    head: (usize, usize),
    pub(super) dragging: bool,
    /// `V`: whole rows, whatever the anchor's and head's columns.
    pub(super) linewise: bool,
}

impl Selection {
    #[cfg(test)]
    pub(crate) fn anchor(&self) -> (usize, usize) {
        self.anchor
    }

    pub(super) fn start(at: (usize, usize)) -> Self {
        Self { anchor: at, head: at, dragging: true, linewise: false }
    }

    pub(super) fn finished(anchor: (usize, usize), head: (usize, usize)) -> Self {
        Self { anchor, head, dragging: false, linewise: false }
    }

    pub(super) fn set_head(&mut self, head: (usize, usize)) {
        self.head = head;
    }

    pub(super) fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }

    pub(super) fn is_empty(&self) -> bool {
        !self.linewise && self.anchor == self.head
    }

    /// Columns of `row` covered by the selection, if any.
    pub(super) fn columns_on(&self, row: usize, row_width: usize) -> Option<Range<usize>> {
        let (a, b) = self.ordered();
        if row < a.0 || row > b.0 {
            return None;
        }
        let start = if row == a.0 && !self.linewise { a.1 } else { 0 };
        let end = if row == b.0 && !self.linewise { b.1 + 1 } else { row_width };
        (start < end).then_some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_span_rows_in_either_drag_direction() {
        let sel = Selection::finished((5, 2), (3, 5));
        assert_eq!(sel.columns_on(2, 80), None);
        assert_eq!(sel.columns_on(3, 80), Some(5..80));
        assert_eq!(sel.columns_on(4, 80), Some(0..80));
        assert_eq!(sel.columns_on(5, 80), Some(0..3));
    }

    #[test]
    fn linewise_covers_whole_rows_whatever_the_columns() {
        let mut sel = Selection::start((3, 5));
        sel.linewise = true;
        assert!(!sel.is_empty(), "one row is already a selection");
        sel.set_head((4, 1));
        assert_eq!(sel.columns_on(3, 80), Some(0..80));
        assert_eq!(sel.columns_on(4, 80), Some(0..80));
        assert_eq!(sel.columns_on(5, 80), None);
    }
}
