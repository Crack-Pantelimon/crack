# Plan 33 — Serena MCP shakedown: findings & root causes

Investigation of the four live test chats from `_slop/plan-32/test-prompts/*`, run in
fresh chats against the running `crack-dev` container. Goal: turn the observed
friction into concrete fixes. **No live chats were started**; all page reads were via
the chromium MCP, all shell via `docker exec crack-dev`.

Chats reviewed:

| id | model | prompt | outcome |
|----|-------|--------|---------|
| [1784833546394](http://localhost:9847/chats/1784833546394) | nemotron-3-ultra-550b | Rust: `find_symbol` + `find_referencing_symbols` on `TreeMapTile` | eventually succeeded after ~8 failed calls |
| [1784833959459](http://localhost:9847/chats/1784833959459) | stepfun step-3.5-flash | Python: `get_symbols_overview` on `model_latency.py` | **failed** — Python LS not enabled |
| [1784834159757](http://localhost:9847/chats/1784834159757) | z-ai/glm-5.2 | Rust: edit `spawn_root_map_tiles`, then revert | succeeded, but **git unusable** |
| [1784834772658](http://localhost:9847/chats/1784834772658) | stepfun step-3.7-flash | same edit/revert | succeeded slowly |

---

## Problem 1 — Weak models can't drive the `mcp` tool

The single biggest source of wasted turns. The pi `mcp` tool is a multi-verb gateway
and the small models guess the shape wrong repeatedly. Observed failure modes:

**1a. Args placed as siblings of `tool` instead of nested under `args`.** Nemotron
(chat 1) called `mcp` **six times** like:

```
{'name_path_pattern': 'TreeMapTile', 'include_body': 'True', 'tool': 'serena_find_symbol'}
```

The adapter only reads the `args` field, so serena received `{}` →
`Field required [type=missing] name_path_pattern`. It never self-corrected from the
error text (one retry burned **124 s** of thinking). It only succeeded once it stumbled
onto the correct form:

```
{'tool': 'serena_find_symbol', 'args': '{"name_path_pattern": "TreeMapTile", "include_body": true}'}
```

**1b. `args` must be JSON-valid.** Nemotron then sent `"include_body": True` (Python
bool) inside the `args` string → `Invalid args JSON: Unexpected token 'T'`. Booleans
must be lowercase `true`/`false`. (`args` accepts either a real JSON object **or** a
JSON string — glm-5.2 in chat 3 used a nested object and it worked.)

**1c. The connect/list/describe verbs are undiscovered.** step-3.7 (chat 4) opened with
`{'search': 'serena', 'includeSchemas': True}` → "No tools matching serena", then
`{'action': 'server'}` to list servers, then `{'connect': 'serena'}`. Serena is **not
auto-connected** — every fresh pi session shows `serena (not connected)` and the model
must issue `{connect: "serena"}` first (the error card literally says
`Use mcp({ connect: "serena" }) or /mcp reconnect serena`).

**1d. Tool names are `serena_`-prefixed.** step-3.5 (chat 2) called
`get_symbols_overview` → "Tool not found. Server serena has: serena_get_symbols_overview, …".

The verified `mcp` contract (from the chats' own error text):

| intent | call |
|--------|------|
| list servers | `{}` or `{"action":"server"}` |
| connect + list a server's tools | `{"connect":"serena"}` |
| show one tool's schema | `{"describe":"serena_find_symbol"}` |
| **call a tool** | `{"server":"serena","tool":"serena_find_symbol","args":{…}}` |

This is exactly the class of problem the hash-anchored edit-tool guidance in
`.pi/SYSTEM.md` already solved for weak models — the fix is the same shape: a dedicated
section describing the contract and giving full worked runs. → **Plan §1.**

## Problem 2 — Python symbol tools are disabled

step-3.5 (chat 2) got the `mcp` contract right and still failed:

```
Error executing tool get_symbols_overview: ValueError:
  Cannot extract symbols from file .pi/crack/server/src/crack_server/model_latency.py.
  Active language servers: ['rust']
```

Root cause: [`.serena/project.yml`](../../.serena/project.yml#L36-L37) declares
`language_servers: [rust]` only. Serena's default Python backend is **pyright**
(`solidlsp` `Language.PYTHON` → `PyrightServer`; alternatives `python_jedi`, `ty`,
`pyrefly`, `basedpyright` all present). Pyright is auto-downloaded into
`/root/.serena/language_servers` (root volume, persists). Enabling it is a one-line
config change plus a decision about the Python **project root / venv** so cross-file
references resolve (the crack-pi-server venv lives at
`target/python-venvs/crack-pi-server-*`). → **Plan §2.**

## Problem 3 — git is unusable inside the sandbox

glm-5.2 (chat 3) tried to use git to verify its edit and got:

```
$ git status --porcelain src/.../map_lod.rs
?? crack_demo/.../map_lod.rs                # <-- untracked
$ git rev-parse HEAD
fatal: ambiguous argument 'HEAD': unknown revision or path not in the working tree
```

It concluded "the file is not git-tracked" and fell back to `sha256sum` for its
byte-identical check. Every git-based workflow (diff, log, blame, stash, revert) is
broken the same way.

Root cause is in [`sandbox.py:materialise_frozen_base`](../../.pi/crack/server/src/crack_server/sandbox.py#L123-L165):
the frozen base is built by `git archive <tree> | tar -x` followed by a bare
`git init`. That leaves a repo with **no HEAD commit and no index** — so every file
reads as untracked and `HEAD` doesn't resolve. The `.git/objects/info/alternates`
pointer to `/crack-host-git-objects` is present (host objects are reachable), but
nothing points HEAD at a commit or seeds the index.

**Verified fix** (prototyped against the real repo objects on crack-dev): after
`git init` + alternates, add

```sh
git update-ref refs/heads/<branch> <HEAD_SHA>   # HEAD_SHA & branch captured at snapshot
git read-tree <HEAD_SHA>                          # seed the index from the frozen tree
```

Result in the prototype: `git status` clean, `git log` shows real history,
`git rev-parse HEAD` returns the real sha, and editing a tracked file shows up in
`git diff --stat` / `git status -s`. Because the snapshot clean-gate guarantees
`write-tree == HEAD^{tree}`, pointing HEAD straight at the host HEAD commit is exact
and needs no new commit object. → **Plan §3.**

## Problem 4 — 2-minute cold index on the first Rust query

The first serena Rust call in a fresh sandbox blocks while rust-analyzer builds
proc-macros and runs `cargo check`. In the chats this landed on the **connect** call
itself (271 s in chat 3, 312 s in chat 4) and on the first `find_symbol` (303 s in
chat 3) — minutes of dead time per new chat.

State today:
- `.mcp.json` serena entry sets `CARGO_TARGET_DIR=/workspace/target/rust-anal` — the
  1.2 GB of cargo artifacts already lands in the **target volume** (`ls` confirms
  `/workspace/target/rust-anal` at 1.2G) and is snapshot-included for sandboxes.
- BUT serena's own document-symbol cache lives at `/workspace/.serena/cache/<lang>`,
  which is **gitignored** ([`.serena/.gitignore`](../../.serena/.gitignore)) → excluded
  from the frozen tree → **empty in every sandbox**, so serena re-queries the LS from
  scratch each time.
- There is no deterministic warm-up: `target/rust-anal` is only as warm as whatever
  manual run last touched it, and nothing guarantees it exists before a snapshot.

The user's ask: warm this **once**, on the crack-dev root container's first boot, in
the background, persisted under the target volume so snapshots inherit it, and guarded
so it doesn't re-run every boot. Serena ships `serena project index <dir>` which does
exactly the LSP-cache population (and drives cargo as a side effect).

Verified enablers for consolidating under `/workspace/target/serena-lang-tools/`:
- `target/` is gitignored, so a cache dir there never pollutes `git status`.
- A **tracked relative symlink** `.serena/cache -> ../target/serena-lang-tools/serena-cache`
  survives `git archive` (confirmed: `lrwxrwxrwx cache -> …`), so the frozen base
  carries the symlink and each sandbox resolves it into the warm target-volume cache.

→ **Plan §4.**

---

## Cross-cutting note

`connect` latency and `find_symbol` latency are the **same** cold-index cost — so the
Plan §4 warm-up directly shrinks the Problem 1 connect stall too, not just later
queries. The four fixes are independent and can land in any order; §1 (docs) and §2
(one-line Python enable) are the cheapest wins.
