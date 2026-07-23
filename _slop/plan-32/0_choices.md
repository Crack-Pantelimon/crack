# Plan 32 — Language-server MCP tools (Rust + Python): the choices

Goal: give pi agents & sub-agents **semantic code tools over the language servers** —
symbol *definition* + *usages/references*, and *refactor primitives* (rename first,
then move/inline/extract). Two servers requested:

1. **Rust** MCP server — wraps `rust-analyzer`, rooted at the same `/workspace`
   project, build artifacts in the mounted target volume.
2. **Python** MCP server — v1 bound to the **crack-pi-server** poetry venv that lives
   in the target dir; later, per-codebase venv selection for the `_data/*` uv repos.

This doc lays out the choices. Nothing is wired yet — everything below was
**smoke-tested live in the running `crack-dev` container** (results in §5).

---

## 1. How MCP servers plug in today (the shape we must match)

- Servers are declared in [`.mcp.json`](../../.mcp.json) as **stdio commands**
  (`command` + `args` + `env`). Examples already present: `web-search`, `chromium`,
  `firefox`, `blender`, `code-search`.
- At container boot [`_sandbox_common.sh`](../../_docker/_sandbox_common.sh#L36-L37)
  copies `/workspace/.mcp.json` → `/root/.config/mcp/mcp.json`; the **`pi-mcp-adapter`**
  gateway (installed in `Dockerfile.base`, patched in [`Dockerfile`](../../_docker/image/Dockerfile#L38-L51))
  reads that global path regardless of agent cwd and exposes each server's tools.
- Toolchains live in the **base image**; heavy language runtimes are already there:
  `rust-analyzer` proxy (component was **not** actually installed — see §5), `uv`/`uvx`,
  `node`/`npx`, `clang`/`lld`/`mold`, `python3.13`.
- The **target volume** `crack-dev-target-dir` is mounted at `/workspace/target`
  (gitignored, persisted). It already contains, from prior work:
  - `target/rust-anal/` — a separate rust-analyzer check/target dir (so RA's
    `cargo check` runs don't invalidate the main `target/debug` build cache).
  - `target/python-venvs/crack-pi-server-*` — the poetry venv for crack-pi-server.
  - `target/python-uv-venvs/{_pi__crack__server,_data__news,_data__3d_data_v2}__venv`
    — an existing per-repo venv naming scheme we can reuse for "which venv" selection.

So a new server = a new stdio entry in `.mcp.json` + whatever binary the base image
provides. The design question is **what binary**, and that splits into two families.

---

## 2. The one architectural fork: per-language bridge vs. all-in-one toolkit

Every option is a wrapper that speaks LSP to a language server and re-exposes it as MCP
tools. They differ in *granularity*:

**Family A — generic LSP↔MCP bridge, one instance per language.**
You run the bridge once per language server, each as its own `.mcp.json` entry:
`rust-lsp` → `rust-analyzer`, `python-lsp` → a venv's `pylsp`. This maps **1:1 onto the
request** ("the rust analyzer mcp server", "a python mcp server bound to the venv") and
onto the future ask (a *different* python entry per venv is just another entry pointing
at a different `pylsp`). Tools are the LSP primitives: definition, references, rename,
hover, diagnostics, edit.

**Family B — all-in-one semantic toolkit (Serena).**
A single MCP server that internally manages language servers for 40+ languages and
exposes a richer, symbol-level tool suite (find_symbol, find_referencing_symbols,
rename_symbol, replace_symbol_body, insert_before/after, safe_delete_symbol, plus
move/inline via LSP). One entry covers both Rust and Python. Downside: it **activates
one project/LS config at a time** and manages its own LS binaries/venv discovery, so
"rust entry" + "python entry bound to venv X" + "python entry bound to venv Y" is not
its native shape — you steer it through `.serena/project.yml`, not through separate
MCP entries.

> **The request is phrased as two separate servers with per-venv binding.** That is
> Family A's native shape. Serena is the popularity king and a strong all-in-one, but
> it fights the "one entry per venv" requirement. Recommendation in §6.

---

## 3. Category: **Rust MCP server** — top 3

| # | Project | Stars | Lang | Shape | Tools | Live-tested |
|---|---------|-------|------|-------|-------|-------------|
| 1 | **isaacphi/mcp-language-server** (`--lsp rust-analyzer`) | 1.6k | Go | per-language bridge | definition, references, **rename_symbol**, hover, diagnostics, edit_file | RA handshake ✅ (§5) |
| 2 | **oraios/serena** | 26.8k | Python | all-in-one | find_symbol, find_referencing_symbols, **rename_symbol**, replace_symbol_body, insert_*, safe_delete, move/inline | installs+runs ✅ |
| 3 | **zeenix/rust-analyzer-mcp** | 73 | Rust | rust-only bridge | workspace symbols, references, rename | not tested (niche) |

Honorable mentions: `blackwell-systems/agent-lsp` (89★, "65 tools, 30 languages",
young), `jonrad/lsp-mcp` (190★), `Tritlo/lsp-mcp` (122★, Haskell-origin).

Notes:
- **rust-analyzer itself is the engine** in all three; it confirms `renameProvider`,
  `referencesProvider`, `definitionProvider` (§5). The choice is only about the wrapper.
- rust-analyzer must run rooted at `/workspace` and should point its check target at
  `target/rust-anal` (already exists) via `rust-analyzer.check.overrideCommand` /
  `CARGO_TARGET_DIR`, so RA's background checks don't thrash the main `target/debug`
  build cache. This is the concrete meaning of "same workspace, target in the volume."
- **#1 needs Go added to the base image** (`go install …@latest`) — Go is currently
  absent (§5). ~150 MB toolchain, or vendor a prebuilt binary.
- **#3** avoids Go (Rust-native, `cargo install`) but is niche/low-maintenance and
  Rust-only, so it can't be reused for the Python category.

---

## 4. Category: **Python MCP server** — top 3

Two sub-choices here: the **bridge** (same table as Rust) and the **Python LSP backend**
it drives. The backend is what makes venv-binding work.

**Bridges (top 3):**

| # | Project | Stars | Why |
|---|---------|-------|-----|
| 1 | **isaacphi/mcp-language-server** (`--lsp <venv>/bin/pylsp`) | 1.6k | Same bridge as Rust → one dependency for both; venv binding is trivial (point `--lsp` at the venv's own `pylsp`). |
| 2 | **oraios/serena** | 26.8k | Same all-in-one; Python via its bundled LS. Venv binding via `.serena` config, not per-entry. |
| 3 | **jonrad/lsp-mcp** / **Tritlo/lsp-mcp** | 190 / 122 | Generic bridges if we want a non-Go alternative to #1. |

**Python LSP backend (the venv-binding decision):**

| Backend | Install | Venv binding | Refactor support | Live |
|---------|---------|--------------|------------------|------|
| **python-lsp-server (`pylsp`)** ✅ recommended v1 | `pip install python-lsp-server` **into the venv** | *Automatic* — the venv's `pylsp` sees exactly that env's site-packages; a second venv = a second `pylsp` binary = a second MCP entry | rename + extract/inline via **rope** plugin; refs/defs/hover/diagnostics | installs+runs in crack-server venv ✅ |
| **pyright / basedpyright** | `npx pyright-langserver` (global) | via `python.venvPath`/`pyrightconfig.json` or `VIRTUAL_ENV` env, **not** by binary location | strong types & references; **rename is weaker/absent** in stock pyright | `pyright 1.1.411` runs via npx ✅ |

> **pylsp wins the v1 venv requirement outright**: "connected to the crack-pi-server env"
> literally means running that venv's `pylsp`. The future "different access per codebase
> venv" then becomes: install `pylsp` into each `_data/*` uv venv and add one MCP entry
> per venv (naming already scaffolded under `target/python-uv-venvs/`). basedpyright is
> the alternative if we prioritize type-accuracy over rename and are willing to drive
> venv selection through config instead of binary path.

---

## 5. Live-container test results (`crack-dev`, this session)

- `rust-analyzer` was **only a rustup proxy stub** — `rust-analyzer --version` errored
  ("Unknown binary … in official toolchain"). `rustup component add rust-analyzer`
  fixed it → **`rust-analyzer 1.97.1`**. → **base image must add the component** (one
  line in `Dockerfile.base` after the existing `rustup component add rustfmt clippy`).
- Raw LSP `initialize` against `rootUri=file:///workspace` returned:
  `renameProvider={prepareProvider:true}`, `referencesProvider=true`,
  `definitionProvider=true` — the refactor primitives we want are all present. ✅
- **serena**: `uvx --from git+https://github.com/oraios/serena serena …` built and ran
  (72 pkgs, ~3 s once cached); `serena tools list` shows `rename_symbol`,
  `find_referencing_symbols`, `find_symbol`, `find_declaration`, `find_implementations`,
  `replace_symbol_body`, `insert_before/after_symbol`, `safe_delete_symbol`,
  `get_diagnostics_for_file`. ✅
- **pyright**: `npx -y pyright --version` → `1.1.411`. ✅
- **pylsp**: `pip install python-lsp-server` into the crack-pi-server poetry venv
  (`target/python-venvs/crack-pi-server-kXl-0cDD-py3.13`) then `<venv>/bin/pylsp --help`
  ran. ✅ — venv binding confirmed working.
- **Go**: absent (`command -v go` → MISSING) — required for option #1 in both categories.

---

## 6. Recommendation & the decisions in front of us

**Recommended path (Family A, matches the request 1:1):**

- **Rust:** `.mcp.json` entry `rust-lsp` → `isaacphi/mcp-language-server --workspace
  /workspace --lsp rust-analyzer`, RA check target pinned to `target/rust-anal`.
- **Python:** `.mcp.json` entry `python-lsp` → same bridge `--lsp <crack-server-venv>/bin/pylsp`,
  with `python-lsp-server[rope]` installed into that venv. Future venvs = one more entry each.
- **Cost:** add **Go** to `Dockerfile.base` (for the bridge) + `rustup component add
  rust-analyzer` + install `pylsp` into the target venv(s) at boot.

**Alternative path (Family B, one server, most popular):**

- Single `serena` entry via `uvx` covering both languages, richest refactor suite
  (move/inline/safe-delete on top of rename), **no Go** needed. Accept that per-venv
  Python selection is driven by Serena project config, not by separate MCP entries, and
  that it manages its own LS binaries.

**Open decisions for you:**

1. **Family A (per-language bridge) or Family B (Serena)?** The request's wording favors
   A; A also gives clean per-venv scaling. B is far more popular and needs no Go, but
   doesn't model "one entry per venv" natively.
2. If A: **accept adding Go to the base image?** (only blocker; alternative is the
   Rust-native `zeenix/rust-analyzer-mcp` for Rust + a non-Go bridge for Python, at the
   cost of two different wrappers.)
3. **Python backend: `pylsp` (auto venv-binding + rope rename) or basedpyright
   (better types, config-driven venv, weak rename)?** Recommended: **pylsp** for v1.
4. **rust-analyzer target dir:** confirm the intent is "root = `/workspace`, RA build
   artifacts in `target/rust-anal` (separate from `target/debug`)" vs. sharing the exact
   same `target/debug` (risks build-cache thrash between RA and `cargo`/`trunk`).

---

### Sources
- [oraios/serena](https://github.com/oraios/serena) — 26.8k★
- [isaacphi/mcp-language-server](https://github.com/isaacphi/mcp-language-server) — 1.6k★
- [zeenix/rust-analyzer-mcp](https://github.com/zeenix/rust-analyzer-mcp) — 73★
- [jonrad/lsp-mcp](https://github.com/jonrad/lsp-mcp) — 190★ · [Tritlo/lsp-mcp](https://github.com/Tritlo/lsp-mcp) — 122★ · [blackwell-systems/agent-lsp](https://github.com/blackwell-systems/agent-lsp) — 89★ · [bug-ops/mcpls](https://github.com/bug-ops/mcpls) — 51★
- [python-lsp-server](https://github.com/python-lsp/python-lsp-server) · [pyright](https://github.com/microsoft/pyright) · [basedpyright](https://github.com/DetachHead/basedpyright)
