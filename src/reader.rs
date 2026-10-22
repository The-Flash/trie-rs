//! Phase 2: query the trie straight from a memory-mapped file.
//!
//! Nothing is deserialized up front. Each lookup reads only the bytes of the
//! nodes on its path, and the OS pages those in on demand.

use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;

use crate::format::{
    self, CHILD_ENTRY_SIZE, FLAG_TERMINAL, HEADER_ROOT_OFFSET, HEADER_SIZE, MAGIC,
    NODE_HEADER_SIZE, VERSION,
};

pub struct DiskTrie {
    mmap: Mmap,
    root: u32,
}

/// What can follow a prefix.
#[derive(Debug, PartialEq, Eq)]
pub struct NextChars {
    /// Characters that can come next, in sorted order.
    pub chars: Vec<char>,
    /// Whether the prefix is itself a complete word.
    pub is_word: bool,
}

/// A view of one node inside the mmap.
struct NodeRef<'a> {
    terminal: bool,
    /// The raw child table: `child_count` entries of (ch: u32, offset: u32).
    children: &'a [u8],
}

impl NodeRef<'_> {
    fn child_count(&self) -> usize {
        self.children.len() / CHILD_ENTRY_SIZE
    }

    fn child_at(&self, i: usize) -> io::Result<(char, u32)> {
        let base = i * CHILD_ENTRY_SIZE;
        let raw = format::read_u32(self.children, base)?;
        let ch = char::from_u32(raw)
            .ok_or_else(|| format::corrupt(format!("invalid char {raw:#x} in child table")))?;
        Ok((ch, format::read_u32(self.children, base + 4)?))
    }

    /// Binary search over the sorted, fixed-width child table.
    fn find_child(&self, target: char) -> io::Result<Option<u32>> {
        let (mut lo, mut hi) = (0, self.child_count());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (ch, offset) = self.child_at(mid)?;
            match ch.cmp(&target) {
                std::cmp::Ordering::Equal => return Ok(Some(offset)),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        Ok(None)
    }
}

impl DiskTrie {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: the mapping is read-only. If another process modified the
        // file while it is mapped, reads could see changed bytes, but every
        // read below is bounds-checked, so that cannot cause memory unsafety.
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_mmap(mmap)
    }

    fn from_mmap(mmap: Mmap) -> io::Result<Self> {
        if mmap.len() < HEADER_SIZE || &mmap[..4] != MAGIC {
            return Err(format::corrupt("not a trie file (bad magic)"));
        }
        let version = format::read_u32(&mmap, 4)?;
        if version != VERSION {
            return Err(format::corrupt(format!("unsupported version {version}")));
        }
        let root = format::read_u32(&mmap, HEADER_ROOT_OFFSET)?;
        Ok(Self { mmap, root })
    }

    pub fn node_count(&self) -> io::Result<u32> {
        format::read_u32(&self.mmap, format::HEADER_NODE_COUNT)
    }

    fn node(&self, offset: u32) -> io::Result<NodeRef<'_>> {
        let off = offset as usize;
        let flags = format::read_u8(&self.mmap, off)?;
        let count = format::read_u32(&self.mmap, off + 1)? as usize;
        let start = off + NODE_HEADER_SIZE;
        let children = count
            .checked_mul(CHILD_ENTRY_SIZE)
            .and_then(|len| start.checked_add(len))
            .and_then(|end| self.mmap.get(start..end))
            .ok_or_else(|| format::corrupt(format!("node at {off} runs past end of file")))?;
        Ok(NodeRef { terminal: flags & FLAG_TERMINAL != 0, children })
    }

    /// Follows `prefix` from the root. Returns the offset of the node it ends
    /// on, or `None` if no word starts with `prefix`.
    fn find_node(&self, prefix: &str) -> io::Result<Option<u32>> {
        let mut offset = self.root;
        for ch in prefix.chars() {
            match self.node(offset)?.find_child(ch)? {
                Some(next) => offset = next,
                None => return Ok(None),
            }
        }
        Ok(Some(offset))
    }

    /// The characters that can follow `prefix`, or `None` if no word starts with it.
    pub fn next_chars(&self, prefix: &str) -> io::Result<Option<NextChars>> {
        let Some(offset) = self.find_node(prefix)? else {
            return Ok(None);
        };
        let node = self.node(offset)?;
        let chars = (0..node.child_count())
            .map(|i| node.child_at(i).map(|(ch, _)| ch))
            .collect::<io::Result<_>>()?;
        Ok(Some(NextChars { chars, is_word: node.terminal }))
    }

    /// Up to `limit` complete words starting with `prefix`, in lexicographic order.
    pub fn complete(&self, prefix: &str, limit: usize) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        if limit == 0 {
            return Ok(out);
        }
        if let Some(offset) = self.find_node(prefix)? {
            let mut word = prefix.to_string();
            self.collect(offset, &mut word, limit, &mut out)?;
        }
        Ok(out)
    }

    /// Depth-first search. Children are sorted, so words come out in order.
    fn collect(
        &self,
        offset: u32,
        word: &mut String,
        limit: usize,
        out: &mut Vec<String>,
    ) -> io::Result<()> {
        let node = self.node(offset)?;
        if node.terminal {
            out.push(word.clone());
        }
        for i in 0..node.child_count() {
            if out.len() >= limit {
                break;
            }
            let (ch, child) = node.child_at(i)?;
            word.push(ch);
            self.collect(child, word, limit, out)?;
            word.pop();
        }
        Ok(())
    }
}
