#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests assert by panicking")]

use std::path::PathBuf;

use super::*;
use crate::store::Location;

fn fixture(tag: &str) -> (PathBuf, Location, Document, Store) {
    let root = std::env::temp_dir().join(format!("plannotator-general-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let location = Location::for_file(&root.join("data"), "project", &root.join("plan.md"));
    let doc = Document::parse("one two three\n\nfour\n".to_owned());
    let store = Store::load(&location, &doc).expect("load empty store");
    (root, location, doc, store)
}

#[test]
fn a_document_has_at_most_one_general_comment_and_saving_again_replaces_its_body() {
    let (root, location, doc, mut store) = fixture("one");
    store.set_general(&doc, "first take".into()).expect("saved");
    store.set_general(&doc, "second take".into()).expect("saved");
    let reloaded = Store::load(&location, &doc).expect("reload");
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded.general().map(|a| a.body.as_str()), Some("second take"));
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn a_general_comment_is_neither_an_orphan_nor_a_passage() {
    let (root, location, doc, mut store) = fixture("not-passage");
    store.set_general(&doc, "overall".into()).expect("saved");
    let edited = Document::parse("entirely different\n".to_owned());
    let reloaded = Store::load(&location, &edited).expect("reload");
    assert_eq!(reloaded.orphans(), 0);
    assert!(reloaded.placed().is_empty());
    assert!(reloaded.general().is_some(), "survives any edit to the document");
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn clearing_the_first_block_keeps_the_general_comment() {
    let (root, _location, doc, mut store) = fixture("clear-block");
    store.set_general(&doc, "overall".into()).expect("saved");
    let first = doc.blocks[0].range.clone();
    assert_eq!(store.remove_starting_in(&doc, &first).expect("cleared"), 0);
    assert!(store.general().is_some());
    std::fs::remove_dir_all(&root).expect("cleanup");
}
