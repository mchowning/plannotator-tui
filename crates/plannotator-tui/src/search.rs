//! Finding a query in a line of rendered text. Vim's smartcase: a query with no capital
//! letter ignores case; one with a capital matches it exactly.

use std::ops::Range;

/// Char ranges in `line` where `query` occurs, in order. Overlapping hits all count, so
/// `n` steps through `aa` in `aaa` twice, as in vim.
pub(crate) fn occurrences(line: &[char], query: &str) -> Vec<Range<usize>> {
    let query: Vec<char> = query.chars().collect();
    if query.is_empty() || query.len() > line.len() {
        return Vec::new();
    }
    let exact = query.iter().any(|c| c.is_uppercase());
    let same = |a: &char, b: &char| if exact { a == b } else { a.to_lowercase().eq(b.to_lowercase()) };
    line.windows(query.len())
        .enumerate()
        .filter(|(_, window)| window.iter().zip(&query).all(|(a, b)| same(a, b)))
        .map(|(start, _)| start..start + query.len())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(line: &str, query: &str) -> Vec<Range<usize>> {
        occurrences(&line.chars().collect::<Vec<_>>(), query)
    }

    #[test]
    fn a_lowercase_query_ignores_case_and_a_capital_makes_it_exact() {
        assert_eq!(find("Plan the plan", "plan"), vec![0..4, 9..13]);
        assert_eq!(find("Plan the plan", "Plan"), vec![0..4]);
    }

    #[test]
    fn overlapping_hits_each_count() {
        assert_eq!(find("aaa", "aa"), vec![0..2, 1..3]);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(find("text", "").is_empty());
    }
}
