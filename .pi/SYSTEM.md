# Using the file tools (read, grep, edit, write) — read this before editing

Your `read`, `edit`, `write`, and `grep` tools are **hash-anchored**. Every
`read`, `grep`, and `write` prints each line as `LINE:HASH|content`, e.g.
`42:ab1|    let x = 1;`. The `LINE:HASH` part (`42:ab1`) is an **anchor**.
Edits target anchors.

## Rule 0 — tool arguments are REAL JSON, never strings

An array is an array; an object is an object. NEVER wrap a nested value in
quotes. This is REJECTED (`edits.0: must be object`):

    "edits": "[{\"set_line\": ...}]"      ← a quoted string, WRONG

This is correct:

    "edits": [ { "set_line": { ... } } ]  ← a real array of real objects

## Rule 1 — anchors come from a read, never invent them

`read` (or `grep`) the file first and copy the exact `LINE:HASH` it printed.
NEVER make up an anchor like `0:0` or `1:0`; only a hash the tool gave you is
valid. If an edit reports `hash-mismatch`, read again and use the fresh hash.

## edit

Shape: `{ "path": "<file>", "edits": [ <edit object>, ... ] }`

Each entry has EXACTLY ONE of these keys (there is no `insert_before`, no
`type`/`line`/`content` key):

- `set_line` — replace one line.
  `{ "set_line": { "anchor": "42:ab1", "new_text": "let x = 2;" } }`
- `replace_lines` — replace a contiguous range.
  `{ "replace_lines": { "start_anchor": "50:c3d", "end_anchor": "55:e4f", "new_text": "a\nb" } }`
- `insert_after` — add text after a line.
  `{ "insert_after": { "anchor": "60:f5a", "new_text": "// note\n" } }`
- `replace` — exact string swap, no anchor needed (fallback).
  `{ "replace": { "old_text": "foo", "new_text": "bar" } }`

`new_text` is plain file content (use `\n` for newlines; `""` deletes the line).

Worked example — after reading and seeing `1:14a|pub const X: u8 = 1;`, bump it
and add a comment after it in one call:

    {
      "path": "src/foo.rs",
      "edits": [
        { "set_line": { "anchor": "1:14a", "new_text": "pub const X: u8 = 2;" } },
        { "insert_after": { "anchor": "1:14a", "new_text": "// bumped\n" } }
      ]
    }

**Inserting at the very top (before line 1):** there is no `insert_before`.
`set_line` the first line, putting your new lines first, then the original line:

    {
      "path": "src/foo.rs",
      "edits": [
        { "set_line": { "anchor": "1:14a", "new_text": "//! Module docs.\n\npub const X: u8 = 1;" } }
      ]
    }

## write — create or fully overwrite a file

    { "path": "src/new.rs", "content": "fn main() {}\n" }

Use `edit` for small changes; `write` replaces the ENTIRE file.

## read — content plus fresh anchors

    { "path": "src/foo.rs" }                              whole small file
    { "path": "src/foo.rs", "offset": 40, "limit": 20 }   lines 40–59
    { "path": "src/foo.rs", "symbol": "main" }            one function/type

## grep — search, returns anchors

    { "pattern": "TODO", "path": "src" }
    { "pattern": "fn main", "literal": true, "glob": "*.rs" }
    { "pattern": "err", "summary": true }                 counts only (broad search)

## todo — structured args, never a string

    { "action": "write", "items": ["Read foo.rs", "Add docs", "Build"] }
    { "action": "toggle", "id": 2 }
    { "action": "list" }
