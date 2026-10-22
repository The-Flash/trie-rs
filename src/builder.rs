//! Phase 1: build the trie in memory, then serialize it to disk.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::format::{self, FLAG_TERMINAL, HEADER_SIZE, MAGIC, VERSION};

#[derive(Default)]
struct Node {
    terminal: bool,
    // BTreeMap keeps children sorted by char, which is exactly the order the
    // on-disk child table needs for binary search.
    children: BTreeMap<char, Node>,
}

pub struct TrieBuilder {
    root: Node,
    node_count: u32,
}

impl Default for TrieBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TrieBuilder {
    pub fn new() -> Self {
        // Start at 1 to count the root.
        Self { root: Node::default(), node_count: 1 }
    }

    pub fn insert(&mut self, word: &str) {
        let mut node = &mut self.root;
        for ch in word.chars() {
            node = node.children.entry(ch).or_insert_with(|| {
                self.node_count += 1;
                Node::default()
            });
        }
        node.terminal = true;
    }

    /// Loads a word list with one word per line. Lines are trimmed and
    /// lowercased, and empty lines are skipped.
    pub fn from_word_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let mut builder = Self::new();
        for line in text.lines() {
            let word = line.trim().to_lowercase();
            if !word.is_empty() {
                builder.insert(&word);
            }
        }
        Ok(builder)
    }

    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    /// Serializes the whole trie into the on-disk byte layout.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();

        // Header: written now with placeholder values, patched at the end
        // once we know where the root landed.
        buf.extend_from_slice(MAGIC);
        format::write_u32(&mut buf, VERSION);
        format::write_u32(&mut buf, 0); // node_count
        format::write_u32(&mut buf, 0); // root_offset
        debug_assert_eq!(buf.len(), HEADER_SIZE);

        let root_offset = write_node(&mut buf, &self.root)?;

        format::patch_u32(&mut buf, format::HEADER_NODE_COUNT, self.node_count);
        format::patch_u32(&mut buf, format::HEADER_ROOT_OFFSET, root_offset);
        Ok(buf)
    }

    pub fn write_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        fs::write(path, self.to_bytes()?)
    }
}

/// Writes `node` post-order: all children first, then the node itself.
/// Because children are already on disk, their offsets are known when the
/// parent's child table is written. Returns the offset of `node`.
///
/// This recurses once per character of the longest word, which is fine for
/// natural-language word lists.
fn write_node(buf: &mut Vec<u8>, node: &Node) -> io::Result<u32> {
    let mut child_offsets = Vec::with_capacity(node.children.len());
    for (&ch, child) in &node.children {
        child_offsets.push((ch, write_node(buf, child)?));
    }

    let offset = u32::try_from(buf.len())
        .map_err(|_| format::corrupt("trie is larger than 4 GB; offsets would overflow u32"))?;

    buf.push(if node.terminal { FLAG_TERMINAL } else { 0 });
    format::write_u32(buf, child_offsets.len() as u32);
    for (ch, child_offset) in child_offsets {
        format::write_u32(buf, ch as u32);
        format::write_u32(buf, child_offset);
    }
    Ok(offset)
}
