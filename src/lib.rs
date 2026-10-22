//! An on-disk trie for prefix completion.
//!
//! - [`TrieBuilder`] builds the trie in memory and writes it to a compact binary file.
//! - [`DiskTrie`] memory-maps that file and answers queries without loading it all.

mod builder;
mod format;
mod reader;

pub use builder::TrieBuilder;
pub use reader::{DiskTrie, NextChars};
