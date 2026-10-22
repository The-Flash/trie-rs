#!/usr/bin/env bash
# Runs the whole flow end to end: build the binary, index word lists, search them.
#
# usage:
#   scripts/demo.sh [words.txt] [prefix ...]
#   scripts/demo.sh --repl [words.txt]      # index, then search interactively
#
# Defaults to /usr/share/dict/words and a few sample prefixes.

set -euo pipefail

cd "$(dirname "$0")/.."

REPL=0
if [[ "${1:-}" == "--repl" ]]; then
    REPL=1
    shift
fi

WORDS="${1:-/usr/share/dict/words}"
shift || true
PREFIXES=("$@")
if [[ ${#PREFIXES[@]} -eq 0 ]]; then
    PREFIXES=(hel tr quee xyzzq)
fi

OUT_DIR=target/demo
BIN=target/release/trie-rs
mkdir -p "$OUT_DIR"

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "1. Build the binary"
cargo build --release --quiet
echo "built $BIN"

# --- Sanity check on a tiny, known word list ---------------------------------

step "2. Index a tiny word list and check the answers"
SMALL="$OUT_DIR/small.txt"
printf 'car\ncat\ndo\n' > "$SMALL"
"$BIN" build "$SMALL" "$OUT_DIR/small.trie"

fail=0
expect() {
    local prefix="$1" pattern="$2" output
    output="$("$BIN" query "$OUT_DIR/small.trie" "$prefix")"
    if grep -q -- "$pattern" <<<"$output"; then
        printf '  ok    %-6s -> %s\n' "\"$prefix\"" "$pattern"
    else
        printf '  FAIL  %-6s expected %q, got:\n%s\n' "\"$prefix\"" "$pattern" "$output"
        fail=1
    fi
}
expect ""    "next letters: cd"
expect "ca"  "next letters: rt"
expect "ca"  "completions:  car, cat"
expect "do"  '"do" is itself a word'
expect "x"   "no words start with"
[[ $fail -eq 0 ]] || { echo "sanity checks failed"; exit 1; }

step "3. The tiny index on disk (layout explained in README.md)"
xxd -c 8 "$OUT_DIR/small.trie"

# --- The real word list ------------------------------------------------------

if [[ ! -f "$WORDS" ]]; then
    echo "word list not found: $WORDS" >&2
    exit 1
fi

TRIE="$OUT_DIR/words.trie"
step "4. Index $WORDS ($(wc -l <"$WORDS" | tr -d ' ') lines)"
"$BIN" build "$WORDS" "$TRIE"

if [[ $REPL -eq 1 ]]; then
    step "5. Search interactively"
    exec "$BIN" repl "$TRIE" --limit 5
fi

step "5. Search"
for prefix in "${PREFIXES[@]}"; do
    printf '\n\033[36m$ trie-rs query %s %s --limit 5\033[0m\n' "$TRIE" "$prefix"
    "$BIN" query "$TRIE" "$prefix" --limit 5
done

step "Done"
echo "Search more:  $BIN query $TRIE <prefix> --limit 10"
echo "Interactive:  $BIN repl $TRIE"
