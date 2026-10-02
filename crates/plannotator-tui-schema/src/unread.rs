//! The unread flag: a thread whose turn ended since the person last opened it, or a note
//! they marked to come back to. Rides in [`Annotation::other`] under [`UNREAD_KEY`], and is
//! absent rather than `false`, so a read annotation is byte-identical to one from before
//! the flag existed.

use serde_json::Value;

use crate::annotation::Annotation;

pub const UNREAD_KEY: &str = "plannotator_tui_unread";

pub fn is_unread(annotation: &Annotation) -> bool {
    annotation.other.get(UNREAD_KEY).and_then(Value::as_bool).unwrap_or(false)
}

pub fn set_unread(annotation: &mut Annotation, unread: bool) {
    if unread {
        annotation.other.insert(UNREAD_KEY.to_owned(), Value::Bool(true));
    } else {
        annotation.other.remove(UNREAD_KEY);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]
mod tests {
    use super::*;

    #[test]
    fn unread_is_on_the_wire_only_while_set() {
        let mut a: Annotation = serde_json::from_value(serde_json::json!({
            "id": "a1", "document_id": "", "anchor": {}, "body": "why?", "author": null,
            "created_at": "", "updated_at": ""
        }))
        .expect("reads");
        let read = serde_json::to_value(&a).expect("serializes");
        assert!(!is_unread(&a));

        set_unread(&mut a, true);
        assert!(is_unread(&a));
        assert_eq!(serde_json::to_value(&a).expect("serializes")[UNREAD_KEY], true);

        set_unread(&mut a, false);
        assert_eq!(serde_json::to_value(&a).expect("serializes"), read, "read again, as before the flag");
    }
}
