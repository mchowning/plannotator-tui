//! Document model: the source text split into top-level Markdown blocks with byte ranges.
//!
//! The parser is `pulldown-cmark`; this module only walks its event stream at depth zero
//! and records where each block starts and ends. Everything downstream (rendering,
//! anchoring, hit-testing) is keyed by block index and byte range. Nothing here knows what
//! a heading or a list *is*.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    Heading,
    Paragraph,
    List,
    CodeBlock,
    BlockQuote,
    Table,
    Rule,
    Html,
    /// YAML front matter; parsed for correct boundaries but never shown.
    Metadata,
    Other,
}

impl BlockKind {
    /// What one of this kind's parts (see `Block::parts`) is called in the footer and prompts.
    pub(crate) fn part_noun(self) -> &'static str {
        match self {
            BlockKind::List => "item",
            _ => "row",
        }
    }

    /// Code and tables keep their columns; everything else word-wraps.
    pub(crate) fn preserves_columns(self) -> bool {
        matches!(self, BlockKind::CodeBlock | BlockKind::Table)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Block {
    pub(crate) range: Range<usize>,
    pub(crate) kind: BlockKind,
    /// What block mode steps through inside this block, in order: a table's header row
    /// and body rows, or a list's top-level items (each with any list nested in it).
    /// Empty for every other kind.
    pub(crate) parts: Vec<Range<usize>>,
}

#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) source: String,
    pub(crate) blocks: Vec<Block>,
}

/// The option set `tui-markdown` enables, so block boundaries agree with its renderer.
fn parse_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
        | Options::ENABLE_MATH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_GFM
        | Options::ENABLE_TABLES
}

impl Document {
    pub(crate) fn parse(source: String) -> Self {
        let blocks = split_blocks(&source);
        Self { source, blocks }
    }

    /// Source text of block `index`. Empty for an out-of-range index.
    pub(crate) fn block_text(&self, index: usize) -> &str {
        self.blocks.get(index).map_or("", |b| &self.source[b.range.clone()])
    }

    /// Source ranges of block `index`'s parts (see `Block::parts`).
    pub(crate) fn parts(&self, index: usize) -> &[Range<usize>] {
        self.blocks.get(index).map_or(&[], |b| &b.parts)
    }

    /// The block whose range contains `offset`.
    pub(crate) fn block_containing(&self, offset: usize) -> Option<usize> {
        self.blocks.iter().position(|b| b.range.contains(&offset))
    }
}

fn kind_of(tag: &Tag<'_>) -> BlockKind {
    match tag {
        Tag::Heading { .. } => BlockKind::Heading,
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::List(_) => BlockKind::List,
        Tag::CodeBlock(_) => BlockKind::CodeBlock,
        Tag::BlockQuote(_) => BlockKind::BlockQuote,
        Tag::Table(_) => BlockKind::Table,
        Tag::HtmlBlock => BlockKind::Html,
        Tag::MetadataBlock(_) => BlockKind::Metadata,
        _ => BlockKind::Other,
    }
}

/// Walk the event stream and cut the source at depth-zero block boundaries.
fn split_blocks(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<(usize, BlockKind)> = None;
    let mut parts = Vec::new();

    for (event, range) in Parser::new_ext(source, parse_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    open = Some((range.start, kind_of(&tag)));
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 1 && matches!(end, TagEnd::TableHead | TagEnd::TableRow | TagEnd::Item) {
                    parts.push(range);
                } else if depth == 0
                    && let Some((start, kind)) = open.take()
                {
                    let parts = std::mem::take(&mut parts);
                    blocks.push(Block { range: start..range.end, kind, parts });
                }
            }
            Event::Rule if depth == 0 => {
                blocks.push(Block { range, kind: BlockKind::Rule, parts: Vec::new() });
            }
            // Any other depth-zero leaf (rare: stray html/text) becomes its own block.
            _ if depth == 0 => blocks.push(Block { range, kind: BlockKind::Other, parts: Vec::new() }),
            _ => {}
        }
    }

    // Trailing newlines are not part of a block's text: quotes stay stable across files
    // that differ only in final-newline conventions.
    let trim = |range: &mut Range<usize>| {
        let trimmed = source[range.clone()].trim_end_matches(['\n', '\r']);
        range.end = range.start + trimmed.len();
    };
    for block in &mut blocks {
        trim(&mut block.range);
        block.parts.iter_mut().for_each(trim);
    }
    blocks.retain(|b| !b.range.is_empty() && b.kind != BlockKind::Metadata);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_top_level_blocks_with_ranges() {
        let src = "# Title\n\nPara one\nstill one.\n\n- a\n- b\n\n```rs\nfn x() {}\n```\n\n---\n";
        let doc = Document::parse(src.to_owned());
        let kinds: Vec<_> = doc.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::List,
                BlockKind::CodeBlock,
                BlockKind::Rule
            ]
        );
        assert_eq!(doc.block_text(0), "# Title");
        assert_eq!(doc.block_text(1), "Para one\nstill one.");
        assert_eq!(doc.block_text(3), "```rs\nfn x() {}\n```");
    }

    #[test]
    fn a_table_records_its_header_and_each_body_row() {
        let doc =
            Document::parse("| Name | Age |\n|---|---|\n| Ann | 30 |\n| Bob | 41 |\n\nafter\n".to_owned());
        let rows: Vec<_> = doc.parts(0).iter().filter_map(|r| doc.source.get(r.clone())).collect();
        assert_eq!(rows, ["| Name | Age |", "| Ann | 30 |", "| Bob | 41 |"]);
        assert!(doc.parts(1).is_empty(), "only tables have rows");
    }

    #[test]
    fn a_list_records_each_top_level_item_with_its_nested_items_inside() {
        let doc = Document::parse("- one\n- two\n  - nested\n- three\n".to_owned());
        let items: Vec<_> = doc.parts(0).iter().filter_map(|r| doc.source.get(r.clone())).collect();
        assert_eq!(items, ["- one", "- two\n  - nested", "- three"]);
    }

    #[test]
    fn front_matter_is_dropped_and_nested_lists_stay_one_block() {
        let doc = Document::parse("---\ntitle: X\n---\n\n- a\n  - nested\n- b\n".to_owned());
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks.first().map(|b| b.kind), Some(BlockKind::List));
    }
}
