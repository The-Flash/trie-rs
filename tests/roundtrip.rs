use std::path::PathBuf;

use trie_rs::{DiskTrie, NextChars, TrieBuilder};

/// A temp file path that is removed when dropped.
struct TempPath(PathBuf);

impl TempPath {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("trie-rs-{}-{name}", std::process::id()));
        Self(path)
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn build(name: &str, words: &[&str]) -> (TempPath, DiskTrie) {
    let mut builder = TrieBuilder::new();
    for w in words {
        builder.insert(w);
    }
    let path = TempPath::new(name);
    builder.write_to(&path.0).unwrap();
    let trie = DiskTrie::open(&path.0).unwrap();
    (path, trie)
}

const WORDS: &[&str] = &["car", "cart", "care", "cat", "dog", "do", "café"];

#[test]
fn next_chars_lists_sorted_children() {
    let (_p, trie) = build("next", WORDS);
    assert_eq!(
        trie.next_chars("ca").unwrap(),
        Some(NextChars { chars: vec!['f', 'r', 't'], is_word: false })
    );
    assert_eq!(
        trie.next_chars("car").unwrap(),
        Some(NextChars { chars: vec!['e', 't'], is_word: true })
    );
    assert_eq!(
        trie.next_chars("caf").unwrap(),
        Some(NextChars { chars: vec!['é'], is_word: false })
    );
}

#[test]
fn leaf_word_has_no_next_chars() {
    let (_p, trie) = build("leaf", WORDS);
    assert_eq!(
        trie.next_chars("cat").unwrap(),
        Some(NextChars { chars: vec![], is_word: true })
    );
}

#[test]
fn missing_prefix_returns_none() {
    let (_p, trie) = build("missing", WORDS);
    assert_eq!(trie.next_chars("x").unwrap(), None);
    assert_eq!(trie.next_chars("cab").unwrap(), None);
    assert!(trie.complete("cab", 10).unwrap().is_empty());
}

#[test]
fn empty_prefix_lists_first_letters() {
    let (_p, trie) = build("empty", WORDS);
    assert_eq!(trie.next_chars("").unwrap().unwrap().chars, vec!['c', 'd']);
}

#[test]
fn complete_is_sorted_and_respects_limit() {
    let (_p, trie) = build("complete", WORDS);
    assert_eq!(
        trie.complete("ca", 10).unwrap(),
        vec!["café", "car", "care", "cart", "cat"]
    );
    assert_eq!(trie.complete("ca", 2).unwrap(), vec!["café", "car"]);
    assert_eq!(trie.complete("do", 10).unwrap(), vec!["do", "dog"]);
    assert!(trie.complete("ca", 0).unwrap().is_empty());
}

#[test]
fn bad_magic_is_rejected() {
    let path = TempPath::new("badmagic");
    std::fs::write(&path.0, b"NOPE\x01\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    let err = DiskTrie::open(&path.0).err().expect("should fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn truncated_file_errors_instead_of_panicking() {
    let mut builder = TrieBuilder::new();
    builder.insert("hello");
    let mut bytes = builder.to_bytes().unwrap();
    // Point the root past the end of the file.
    bytes[12..16].copy_from_slice(&9999u32.to_le_bytes());
    let path = TempPath::new("truncated");
    std::fs::write(&path.0, bytes).unwrap();
    let trie = DiskTrie::open(&path.0).unwrap();
    assert!(trie.next_chars("h").is_err());
}
