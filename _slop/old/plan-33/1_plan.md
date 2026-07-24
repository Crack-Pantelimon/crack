# Plan 33 — implementation plan

Four independent workstreams. Ordered cheapest-first. File references are to the repo
root. Everything below was feasibility-checked in the running container (see
[`0_report.md`](0_report.md)); nothing here is wired yet.

---

## §1 — Teach weak models the `mcp`/serena tool contract in `.pi/SYSTEM.md`

`.pi/SYSTEM.md` is loaded for weak coder/planner models via
`_build_cmd --append-system-prompt` (see memory `pi-system-md-tool-guidance`). Add a new
section after the existing edit-tool guidance, mirroring its "contract + gotchas + full
worked run" style. It must nail the four failure modes from Problem 1.

**Content to add** (a `## Using MCP tools (serena, chromium, …)` section):

1. **The gateway shape.** The `mcp` tool is a gateway with verbs:
   - `{}` → list servers and connection state.
   - `{"connect":"serena"}` → connect a server and list its tools. **Servers start
     disconnected — you must connect once per session before calling any tool.**
   - `{"describe":"serena_find_symbol"}` → print one tool's parameter schema.
   - `{"server":"serena","tool":"serena_find_symbol","args":{ … }}` → **call** a tool.
2. **Rule (the #1 mistake):** the tool's own parameters go **inside `args`**, never as
   siblings of `tool`. Show the exact wrong/right pair from chat 1:
   - WRONG: `{"tool":"serena_find_symbol","name_path_pattern":"Foo"}` → server sees `{}`.
   - RIGHT: `{"tool":"serena_find_symbol","args":{"name_path_pattern":"Foo"}}`.
3. **Rule:** `args` is JSON — booleans are `true`/`false`, **not** Python `True`.
4. **Rule:** serena tool names are `serena_`-prefixed
   (`serena_get_symbols_overview`, not `get_symbols_overview`).
5. **Describe every serena tool** (the 21 from the connect listing), grouped:
   - *Read/navigate:* `serena_get_symbols_overview`, `serena_find_symbol`,
     `serena_find_referencing_symbols`, `serena_find_implementations`,
     `serena_find_declaration`, `serena_get_diagnostics_for_file`.
   - *Edit:* `serena_replace_symbol_body`, `serena_insert_after_symbol`,
     `serena_insert_before_symbol`, `serena_rename_symbol`, `serena_safe_delete_symbol`,
     `serena_replace_content`, `serena_replace_in_files`.
   - *Memory (rarely needed for one-off tasks — say so):* the `serena_*_memory` /
     `serena_onboarding` / `serena_initial_instructions` set.
   For each, one line + required params (mirror the `describe` output, e.g.
   `find_symbol` needs `name_path_pattern`; `find_referencing_symbols` needs
   `name_path` **and** `relative_path`; overview needs `relative_path`).
6. **Two full worked runs** (like the edit examples already in the file):
   - *Rust definition + references* — the exact successful sequence from chat 1:
     ```
     mcp {"connect":"serena"}
     mcp {"server":"serena","tool":"serena_find_symbol",
          "args":{"name_path_pattern":"TreeMapTile","include_body":true}}
     # note relative_path from the result, then:
     mcp {"server":"serena","tool":"serena_find_referencing_symbols",
          "args":{"name_path":"TreeMapTile",
                  "relative_path":"crack_demo/.../map_lod.rs"}}
     ```
   - *Symbol-safe edit + revert* — the chat 3/4 flow: `find_symbol` with
     `include_body:true`, `replace_symbol_body`, then `replace_symbol_body` again to
     restore. Stress: capture the original `body` string before editing so revert is
     exact.

**Files:** [`.pi/SYSTEM.md`](../../.pi/SYSTEM.md) (append section). No code changes.
Keep it as terse and example-driven as the edit section; weak models copy examples.

---

## §2 — Enable Python symbol tools

**2a. Config (required).** In [`.serena/project.yml`](../../.serena/project.yml#L36-L37):

```yaml
language_servers:
- rust
- python
```

Serena will auto-download pyright into `/root/.serena/language_servers` on first use
(root volume, persists across `run.sh` recreate). Multi-LS is supported — "the first
language server that supports a given file is used", so rust + python coexist.

**2b. Python project root / venv (the "relative root dir" question).** For
`get_symbols_overview` / `find_symbol` / `find_referencing_symbols` on
`.pi/crack/server/src/crack_server/*.py`, pyright analyses files rooted at `/workspace`
already (serena `ls_workspace_folders: ["."]`). That is enough for the failed chat-2
task. For **accurate cross-package references** into installed deps, point pyright at
the crack-pi-server venv via `ls_specific_settings` in `project.yml`:

```yaml
ls_specific_settings:
  python:
    # pyright resolves imports against this interpreter's site-packages
    python:
      pythonPath: "/workspace/target/python-venvs/<crack-pi-server-venv>/bin/python"
```

The venv dir name is resolved at boot (glob `target/python-venvs/crack-pi-server-*`);
prefer wiring this in `_cont_start.sh` (which already runs `poetry install` in
`/workspace/.pi/crack/server`) rather than hard-coding the hashed venv name. If the
future per-repo-venv requirement returns, this is the knob to templatize.

**2c. Validate** (via `docker exec`, no live chat): after config change,
`serena project index /workspace` should report python files indexed, and a manual
`get_symbols_overview relative_path=.pi/crack/server/src/crack_server/model_latency.py`
should return symbols instead of `Active language servers: ['rust']`.

**Decision for the user:** default **pyright** (types + refs, weak rename) vs
`python_jedi` (lighter, no venv needed, good nav). Recommend **pyright** to match the
Plan-32 §4 analysis; jedi is the fallback if pyright's download/venv wiring proves
fussy in the sandbox.

---

## §3 — Make git work in the sandbox

Fix [`materialise_frozen_base`](../../.pi/crack/server/src/crack_server/sandbox.py#L123-L165)
so the frozen base is a real repo with a HEAD commit and a seeded index.

**3a. Capture HEAD + branch at snapshot time.** Extend
[`snapshot_host_tree`](../../.pi/crack/server/src/crack_server/sandbox.py#L105-L120)
(or add a sibling helper) to also return the host `HEAD` sha and current branch:

```python
head_sha = git("-C", repo, "rev-parse", "HEAD")
branch   = git("-C", repo, "symbolic-ref", "--short", "HEAD")  # fallback: "master"
```

Thread these into `materialise_frozen_base(...)` (and persist alongside `tree`, next to
the existing `overlays/tree` file, so sub-agents reusing a parent base can replay them).

**3b. After `git init` + writing `alternates`, seed HEAD and the index:**

```python
subprocess.run(["git", "-C", str(dest), "symbolic-ref", "HEAD", f"refs/heads/{branch}"], check=True)
subprocess.run(["git", "-C", str(dest), "update-ref", f"refs/heads/{branch}", head_sha], check=True)
subprocess.run(["git", "-C", str(dest), "read-tree", head_sha], check=True)
```

The host HEAD commit and its whole ancestry are reachable through the alternates
(`/crack-host-git-objects`), so no new object is written and `git log` shows real
history. The clean-gate invariant (`write-tree == HEAD^{tree}`) guarantees the seeded
index matches the extracted working tree → `git status` is clean.

**Verified prototype** (against `/workspace/.git/objects` on crack-dev):
`git status` clean · `git log --oneline` real history · `git rev-parse HEAD` real sha ·
editing a tracked file → `git diff --stat` and `git status -s` both report it.

**3c. Edge cases to keep:**
- Idempotency guard at the top of `materialise_frozen_base` stays as-is (re-ensure is a
  no-op).
- If `branch` can't be determined (detached HEAD), fall back to `master` for the ref
  name; HEAD still points at the sha so everything resolves.
- Sub-agents (`parent_conv` path) already reuse the parent's base dir — they inherit
  the fixed `.git` for free; just make sure the head/branch file is copied like `tree`.

**Tests:** extend `.pi/crack/server/tests/test_sandbox.py` — after materialising, assert
`git -C base rev-parse HEAD` succeeds and `git -C base status --porcelain` is empty.

---

## §4 — One-time boot warm-up, persisted under the target volume

Goal: after the crack-dev **root** container's first boot, `target/` already holds a
warm rust (and python) index, so the first sandbox query is fast and `connect` no
longer stalls for minutes.

**4a. Consolidate serena state under `/workspace/target/serena-lang-tools/`.**
- Keep/point `CARGO_TARGET_DIR` at `…/serena-lang-tools/rust-anal` (move the existing
  `target/rust-anal`, or symlink for one release). Update the `serena` env block in
  [`.mcp.json`](../../.mcp.json#L71-L74).
- Relocate serena's document-symbol cache into the volume via a **tracked relative
  symlink** so it is snapshot-included:
  - `git rm`-style: remove `/cache` from [`.serena/.gitignore`](../../.serena/.gitignore),
  - `ln -s ../target/serena-lang-tools/serena-cache .serena/cache` and commit the symlink.
  - Confirmed: `git archive` carries the symlink into the frozen base, and `target/`
    stays gitignored so the cache never shows in `git status`.

**4b. Background, guarded warm-up in `_cont_start.sh`.** Mirror the existing
claude-context bring-up block
([`_cont_start.sh:102-113`](../../_docker/_cont_start.sh#L102-L113)):

```sh
# --- serena LSP warm-up (once, persisted in target volume) ------------------
SERENA_WARM=/workspace/target/serena-lang-tools/.indexed
mkdir -p /workspace/target/serena-lang-tools/serena-cache
if [ ! -f "$SERENA_WARM" ]; then
  (
    set +e
    CARGO_TARGET_DIR=/workspace/target/serena-lang-tools/rust-anal \
      /root/.local/bin/serena project index /workspace --log-level ERROR \
      && touch "$SERENA_WARM"
  ) >>"$CRACK_HARNESS_DATA_DIR/harness/mcp-http/serena-warmup.log" 2>&1 &
fi
# ----------------------------------------------------------------------------
```

- Runs detached so `crack-server` startup never blocks on it (same discipline as the
  RAG index).
- The `.indexed` marker lives in the target volume → survives `run.sh --force-recreate`
  and is snapshot-included, so it is genuinely **one-time**, not per-boot.
- `serena project index` populates the LSP cache for the configured languages (rust +,
  after §2, python) and drives `cargo check` into `CARGO_TARGET_DIR` as a side effect.

**4c. Invalidation.** The marker makes warm-up skip forever once done. Add a comment
that deleting `target/serena-lang-tools/.indexed` (or bumping a version suffix in the
marker name when the crate graph changes materially) forces a re-warm. Optional refinement:
name the marker after a hash of `Cargo.lock` so a dependency bump re-warms automatically.

**Note / cleanup:** serena uses its **own** downloaded rust-analyzer
(`/root/.serena/language_servers/static/RustAnalyzer`), not the rustup component — so
the `rustup component add rust-analyzer` line in
[`_cont_start.sh:14`](../../_docker/_cont_start.sh#L14) is not needed by serena. Leave it
only if another tool relies on it; otherwise drop it to save a boot step.

---

## Suggested landing order

1. **§1** SYSTEM.md docs — pure docs, immediate ROI for every weak-model chat.
2. **§2** Python enable — one config line + optional venv wiring.
3. **§3** git fix — small, well-scoped `sandbox.py` change + a test.
4. **§4** warm-up + target consolidation — the most moving parts (compose/env/symlink);
   depends on §2 so the warm-up also indexes Python.

Each is independently shippable and independently testable via `docker exec`
(no live chats required).
