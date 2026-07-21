# Implementation Report: `game_logic` documentation (worker models fix)

## Summary

Resolved the remaining `missing_docs` / `unused_doc_comments` warnings in
`crack_demo/game_logic` for the `worker` feature. Plain `cargo build` already
reported **0** `missing documentation` lines; `cargo build --features worker`
had **4** doc-related warnings (6 total with duplicates), all from
`src/worker/models.rs` and `declare_model_group!`.

After the fix, both builds report **0** `missing documentation` and **0**
`unused doc comment` warnings. `./test.sh` passes (32 unit tests across native
default, native worker, and wasm).

## Root cause

`declare_model_group!` in `storage_crackhouse` generates public structs and
fields but does not forward `///` comments from the macro invocation. A `///`
above the invocation is an **unused doc comment**; docs inside the invocation
are rejected by the macro matcher (no `doc` token rule).

The only fix within `game_logic` (without editing `storage_crackhouse`) is to
inline the macro expansion and attach `///` docs directly on the generated items.

## Files changed (this session)

| File | Change |
|------|--------|
| `src/worker/models.rs` | Replaced `declare_model_group!` with its manual expansion plus `///` docs on `GameLogicModels`, `GameKvEntry_Entity`, `GameKvEntry`, and fields `id` / `val`. Removed unused `declare_model_group` import. Kept `run_game_migrations` doc. |

**Note:** `models.rs` is not a doc-only diff — it inlines macro-generated code so
docs can attach to the public items. Behavior is unchanged (same table schema,
traits, and migration SQL). Prior agents had already documented the rest of the
crate and added `[lints] workspace = true` in `Cargo.toml`.

## Documentation added in `models.rs`

- `GameLogicModels` — storage model group marker
- `GameKvEntry_Entity` — entity metadata for the table schema
- `GameKvEntry` — key-value row struct
- `GameKvEntry::id` — row primary key
- `GameKvEntry::val` — optional string payload
- `run_game_migrations` — already present; retained

## Build & test

```bash
cd /workspace/crack_demo/game_logic

# Default features
cargo build 2>&1 | rg "missing documentation|unused doc" | wc -l   # 0

# Worker feature (same as test.sh second cargo test)
cargo build --features worker 2>&1 | rg "missing documentation|unused doc" | wc -l   # 0

./test.sh   # passes
```

**Final warning count:** **0** doc-related warnings attributable to
`crack_demo/game_logic` on both `cargo build` and `cargo build --features worker`.

## Follow-ups

- Consider extending `declare_model_group!` in `storage_crackhouse` to accept
  `///` on structs/fields inside the invocation and emit them in the expansion,
  so future model groups stay concise and doc-only at call sites.
- A human pass could tighten wording on auto-generated docs elsewhere in the
  crate (from prior agents); this session only touched `worker/models.rs`.
