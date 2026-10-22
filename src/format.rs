//! The on-disk layout shared by the builder and the reader.
//!
//! All integers are little-endian.
//!
//! ```text
//! Header (16 bytes)
//!   magic        [u8; 4]  = b"TRIE"
//!   version      u32      = 1
//!   node_count   u32
//!   root_offset  u32      // byte offset of the root node
//!
//! Node (variable length, at any offset)
//!   flags        u8       // bit 0 = this node ends a word
//!   child_count  u32
//!   children     [ (ch: u32, offset: u32) ; child_count ]   // sorted by ch
//! ```
//!
//! Child entries are fixed width (8 bytes), so a node's children form a
//! sorted array that can be binary searched directly inside the mmap.

use std::io;

pub const MAGIC: &[u8; 4] = b"TRIE";
pub const VERSION: u32 = 1;

pub const HEADER_SIZE: usize = 16;
pub const HEADER_NODE_COUNT: usize = 8;
pub const HEADER_ROOT_OFFSET: usize = 12;

/// flags (1 byte) + child_count (4 bytes), before the child table starts.
pub const NODE_HEADER_SIZE: usize = 5;
/// ch (4 bytes) + offset (4 bytes).
pub const CHILD_ENTRY_SIZE: usize = 8;

pub const FLAG_TERMINAL: u8 = 0b0000_0001;

pub fn corrupt(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Reads a `u32` at `off`, failing instead of panicking if it runs past the end.
pub fn read_u32(buf: &[u8], off: usize) -> io::Result<u32> {
    let bytes = off
        .checked_add(4)
        .and_then(|end| buf.get(off..end))
        .ok_or_else(|| corrupt(format!("u32 at offset {off} is out of bounds")))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub fn read_u8(buf: &[u8], off: usize) -> io::Result<u8> {
    buf.get(off)
        .copied()
        .ok_or_else(|| corrupt(format!("u8 at offset {off} is out of bounds")))
}

pub fn write_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Overwrites 4 bytes already in `buf` (used to patch the header at the end).
pub fn patch_u32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}
