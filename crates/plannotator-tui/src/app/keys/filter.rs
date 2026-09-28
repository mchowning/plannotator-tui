//! The key list's `/` filter.

/// What `/` narrowed the list to. Rows whose key or description contains `text`, ignoring
/// case, stay; `typing` is true until enter or esc ends the typing.
#[derive(Debug, Default)]
pub(in crate::app) struct KeyFilter {
    pub(super) text: String,
    pub(super) typing: bool,
}

impl KeyFilter {
    pub(super) fn keeps(&self, keys: &str, does: &str) -> bool {
        let needle = self.text.to_lowercase();
        keys.to_lowercase().contains(&needle) || does.to_lowercase().contains(&needle)
    }

    /// The bottom border: what is typed, and how to keep or clear it.
    pub(super) fn help(&self) -> String {
        match (self.typing, self.text.is_empty()) {
            (true, _) => format!(" /{} \u{b7} enter keep \u{b7} esc clear ", self.text),
            (false, true) => super::KEYS_HELP.to_owned(),
            (false, false) => format!(" /{} \u{b7} j/k scroll \u{b7} esc clear ", self.text),
        }
    }
}
