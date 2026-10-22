# trie-rs

A small on-disk trie for prefix completion. Give it a prefix and it returns
the letters that can come next, plus the words that complete it.

```sh
cargo run --release -- build /usr/share/dict/words words.trie
cargo run --release -- query words.trie hel --limit 5
cargo run --release -- repl  words.trie
```

Or run the whole flow (build, sanity checks, index, search) with one script:

```sh
scripts/demo.sh                          # /usr/share/dict/words + sample prefixes
scripts/demo.sh my-words.txt foo ba      # your own word list and prefixes
scripts/demo.sh --repl                   # index, then search interactively
```

## How it works

### 1. Build in memory (`src/builder.rs`)

Each word is inserted into an ordinary pointer-based trie. Every node holds
`terminal: bool` and `children: BTreeMap<char, Node>`. The `BTreeMap` keeps
children sorted, which the disk format relies on.

### 2. Serialize post-order (`src/builder.rs`, `src/format.rs`)

The trie is flattened into one byte buffer. Every child is written **before**
its parent, so when the parent's child table is written, each child's byte
offset is already known. The root is written last. A 16-byte header at the
front records where the root is.

Pointers turn into byte offsets. That is the whole trick that makes a tree
storable on disk. The next section shows the exact bytes.

### 3. Query from the mmap (`src/reader.rs`)

`DiskTrie::open` memory-maps the file and checks only the header. A lookup for
`"hel"` then:

1. reads the root node at `root_offset`
2. binary searches its child table for `'h'`. This works because the entries
   are fixed width (8 bytes) and sorted.
3. jumps to that child's offset, and repeats for `'e'` and `'l'`

The next letters are simply the child table of the final node. For
completions, a depth-first search runs from that node. Since the children are
sorted, words come out in alphabetical order.

The process reads only the pages that hold nodes on the path, so the cost of a
query depends on the prefix length, not on the size of the file. Every read is
bounds-checked, so a corrupt file gives an error rather than a panic.

## On-disk layout

All integers are little-endian. A file is one header followed by nodes packed
back to back, with no padding between them.

### Header (16 bytes, always at offset 0)

| Offset | Size | Field         | Meaning                              |
|-------:|-----:|---------------|--------------------------------------|
| 0      | 4    | `magic`       | the ASCII bytes `TRIE`               |
| 4      | 4    | `version`     | `1`                                  |
| 8      | 4    | `node_count`  | total number of nodes                |
| 12     | 4    | `root_offset` | byte offset of the root node         |

### Node (variable length: `5 + 8 × child_count` bytes)

| Offset in node | Size | Field         | Meaning                                  |
|---------------:|-----:|---------------|------------------------------------------|
| 0              | 1    | `flags`       | bit 0 set = a word ends at this node     |
| 1              | 4    | `child_count` | number of entries in the child table     |
| 5              | 8 each | child table | `(ch: u32, offset: u32)`, sorted by `ch` |

`ch` is the Unicode code point of the edge's character, and `offset` is where
that child node starts in the file. A node doesn't store its own letter; the
letter lives on the edge in the parent's child table.

### Worked example: `car`, `cat`, `do`

The in-memory trie (`*` marks a node where a word ends):

```
        (root)
        /    \
       c      d
       |      |
       a      o*
      / \
     r*  t*
```

Nodes are written post-order (children before their parent), so the root is
written last:

```
offset  0        16   21   26                   47           60   65           78                  99
        +--------+----+----+--------------------+------------+----+------------+-------------------+
        | header | r* | t* | "ca"  [r→16, t→21] | "c" [a→26] | o* | "d" [o→60] | root [c→47, d→65] |
        +--------+----+----+--------------------+------------+----+------------+-------------------+
          16 B    5 B  5 B        21 B              13 B      5 B     13 B            21 B
```

Here is the real file, from `trie-rs build` and `xxd`, split by node:

```
offset  bytes                                         decoded
------  --------------------------------------------  -----------------------------------
   0    54 52 49 45                                    magic = "TRIE"
   4    01 00 00 00                                    version = 1
   8    07 00 00 00                                    node_count = 7
  12    4e 00 00 00                                    root_offset = 78

  16    01 | 00 00 00 00                               "car": terminal, 0 children
  21    01 | 00 00 00 00                               "cat": terminal, 0 children

  26    00 | 02 00 00 00                               "ca":  not terminal, 2 children
        72 00 00 00  10 00 00 00                         'r' (0x72) -> 16
        74 00 00 00  15 00 00 00                         't' (0x74) -> 21

  47    00 | 01 00 00 00                               "c":   not terminal, 1 child
        61 00 00 00  1a 00 00 00                         'a' (0x61) -> 26

  60    01 | 00 00 00 00                               "do":  terminal, 0 children

  65    00 | 01 00 00 00                               "d":   not terminal, 1 child
        6f 00 00 00  3c 00 00 00                         'o' (0x6f) -> 60

  78    00 | 02 00 00 00                               root:  not terminal, 2 children
        63 00 00 00  2f 00 00 00                         'c' (0x63) -> 47
        64 00 00 00  41 00 00 00                         'd' (0x64) -> 65
```

### Following a lookup through the bytes

`next_chars("ca")` reads only three nodes:

1. Read `root_offset` at byte 12, which is **78**.
2. At 78, binary search the 2-entry child table for `'c'`. It is found, and
   points to **47**.
3. At 47, search the table for `'a'`, which points to **26**.
4. At 26, the child table holds `'r'` and `'t'`, so the next letters are
   **`r`, `t`**. `flags` is `00`, so `"ca"` is not a word by itself.

The bytes for `d` and `do` (60 to 77) are never touched. On a large dictionary
this is why a query reads a few hundred bytes out of a ~10 MB file.

You can reproduce the dump yourself:

```sh
printf 'car\ncat\ndo\n' > small.txt
cargo run --release -- build small.txt small.trie
xxd -c 8 small.trie
```

## Ideas to explore next

- **Rank completions.** Store a frequency per word, and keep the max frequency
  of each subtree so the best completions can be found first.
- **Shrink the file.** Merge identical suffix subtrees into a DAWG/FST, and use
  varint offsets instead of fixed `u32`.
- **Locality.** Lay out nodes breadth-first so that the top levels, which every
  query touches, sit together in a few pages.
