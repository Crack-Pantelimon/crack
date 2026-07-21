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

## Gotcha 1 — an edit changes ONLY its target lines; never re-type existing lines

`set_line`, `insert_after`, and `replace_lines` touch **only** the line(s) they
anchor. Every other line in the file stays exactly where it was — nothing shifts,
nothing is removed. So `new_text` must contain **only** the content that is
actually new. Do NOT paste back lines that already exist elsewhere in the file:
they are still there, and pasting them again **duplicates** them.

This is the #1 mistake. It happens when you treat `edit` like `write` and drop a
whole block (or the whole file) into one `set_line`.

WRONG — `set_line` on line 1 with the whole file re-typed. Lines 2..N still
exist below, so every `pub mod` / `const` is now defined **twice** (Rust:
`error[E0428]: defined multiple times`, `error[E0252]: name defined multiple
times`):

    // file already contains, from a read:
    //   1:88f|pub mod chat_const;
    //   2:936|pub mod chat_controller;
    { "set_line": { "anchor": "1:88f",
      "new_text": "//! Docs.\npub mod chat_const;\npub mod chat_controller;" } }
    // → line 1 becomes 3 lines, but old lines 2:936 etc. are UNTOUCHED and remain
    //   ⇒ chat_controller declared twice.

RIGHT — to add a doc line above ONE existing line, `set_line` **that one line**
and repeat **only its own** original text (never its neighbors):

    { "set_line": { "anchor": "2:936",
      "new_text": "/// Chat controller.\npub mod chat_controller;" } }

To add a NEW line that doesn't exist yet, use `insert_after` with just the new
text — do not restate the anchor line:

    { "insert_after": { "anchor": "1:88f", "new_text": "/// Chat controller.\n" } }

After any multi-line `new_text`, glance at the next read: if a definition now
appears twice, you duplicated it — fix by deleting the extra copy
(`set_line` … `new_text: ""`), don't paper over it.

## Gotcha 2 — doc comments attach to the item DIRECTLY below, above attributes

A `///` doc comment documents the very next item, and it must sit **above** that
item's attributes (`#[derive(...)]`, `#[cfg(...)]`) with **no blank line** in
between. Land it in the wrong place and it attaches to the wrong thing or dangles
(Rust warns `unused_doc_comment`, and the item ends up undocumented).

WRONG — doc placed after the attribute, or a second doc added around it. Here the
struct ends up with a doc both above and below the derive (redundant), and a doc
stranded on the `use`:

    /// Node handler.
    use crate::foo::Bar;          // ← doc wrongly attached to a `use`

    /// Sleep manager.
    #[derive(Clone, Debug)]
    /// Sleep manager, again.      // ← second, duplicate doc
    pub struct SleepManager { ... }

RIGHT — one doc block, immediately above the attribute, nothing between it and
the item:

    use crate::foo::Bar;

    /// Sleep manager for interruptible sleeps.
    #[derive(Clone, Debug)]
    pub struct SleepManager { ... }

The reliable move: `set_line` the item's own line (the `#[derive(...)]` line, or
the `pub struct`/`pub fn`/`pub mod` line if it has no attribute) and prepend the
doc to it — `new_text: "/// doc\n<that exact original line>"`. That guarantees
correct placement and touches nothing else.

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
