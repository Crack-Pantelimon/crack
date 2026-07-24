# Plan 5 — Cheap sandboxes: skip the eager MCP boot, keep tools lazy

> Read `0_overview.md`; requires Plans 1–3. Goal: a sandbox at rest costs ~pi+shell, not
> 10 GB. Today `_cont_start.sh` eagerly starts Xvfb + Blender + Playwright/Chrome HTTP
> bridges. Those are only for **host debuggers**; sandboxed pi agents use **stdio MCP** via
> `.mcp.json`, launched lazily per tool call.

## Key facts (verified)

- `.mcp.json` (synced to `/root/.config/mcp/mcp.json` at boot) defines four stdio servers:
  `web-search` (node), `chromium` (npx chrome-devtools-mcp), `firefox` (npx @playwright/mcp),
  `blender` (blender-mcp). The pi mcp-adapter launches these **on first use** — they do NOT
  need the eager `respawn` block.
- The eager `respawn` block in `_cont_start.sh` (ports 9930/9931/9932/9877 + Xvfb + a
  persistent Blender) exists solely to expose the SAME servers over HTTP to a host user. In a
  sandbox (which exposes no ports), it is pure waste and the source of the 10 GB footprint.
- **Blender is the exception:** `blender` stdio MCP connects to a Blender addon socket on
  `:9876` — something must start Blender+Xvfb for it to work. web-search/chromium/firefox
  self-spawn their process on demand and need nothing pre-started.

## Implementation

### 5a. Split the entrypoint by role

`_cont_start.sh` is crack-dev's entrypoint. Sandboxes run `sleep infinity` (Plan 2) and do
**not** run it — good. But sandboxes still need the lazy MCP config present and the shared
env. Two options; pick one:

- **Option A (recommended):** give sandboxes a tiny init that does only the cheap setup
  (env exports + `cp /workspace/.mcp.json /root/.config/mcp/mcp.json` + blender addon sync)
  then `sleep infinity`. Create `_docker/_sandbox_start.sh` with just the top, non-`respawn`
  portion of `_cont_start.sh`. Have `sandbox.ensure_sandbox` run
  `... localhost/crack-dev:latest bash /workspace/_docker/_sandbox_start.sh` instead of raw
  `sleep infinity`. (Since `/root` is an `:O` overlay of crack-dev's `/root`, the mcp.json and
  addon may already be present in the lower — the copy is cheap and idempotent.)
- **Option B:** guard the `respawn` block in `_cont_start.sh` behind
  `if [ "${CRACK_ROLE:-root}" = root ]; then ... fi` and pass `CRACK_ROLE=sandbox` to
  sandboxes. Fewer files, but sandboxes then run the full entrypoint (including `uv sync` +
  `uv run crack-server` — which you must NOT do in a sandbox). Option A is cleaner.

**Do not** start crack-server/worker inside a sandbox — only crack-dev runs those.

### 5b. Blender on demand (only if a sandbox needs 3D)

Keep Blender out of the default sandbox. If/when a task needs it, the `blender` stdio server
will fail to connect to `:9876`. Provide a lazy starter: a wrapper the blender MCP `command`
points to that, on first invocation, boots `Xvfb :99` + `blender --addons blendermcp` inside
the sandbox, waits for `:9876`, then execs blender-mcp. Document it but treat full Blender
support as best-effort for this plan — most sandboxes never touch it.

### 5c. Networking sanity for lazy MCP

The stdio MCP servers run **inside the sandbox** (spawned by pi), so they need no ports and
no `crack-net` exposure. The only cross-container traffic is pi→crack-server (spawn/wait/ask/
vision), already handled by `CRACK_PI_HOST=crack-dev` on `crack-net` (Plan 2). Confirm a
browser tool call inside a sandbox works fully offline-from-host (no published port needed).

## Verification

1. **At-rest cost:** start a sandbox (via the nemotron sample chat, a text-only task). While
   it runs, measure it:
   ```bash
   docker exec crack-dev bash -exc 'podman stats --no-stream --format "{{.Name}} {{.MemUsage}}" | grep crack-sbx'
   ```
   Expect tens–hundreds of MB, **not** multiple GB. Also confirm no Xvfb/Blender/npx bridge
   processes in the sandbox:
   ```bash
   docker exec crack-dev bash -exc 'podman exec <sbx> bash -c "pgrep -fa Xvfb; pgrep -fa blender; pgrep -fa supergateway" || echo "OK: no eager MCP procs"'
   ```
2. **Lazy browser still works:** nemotron chat task "use the web-search tool to find the
   current time in Tokyo and report it, then stop" (or a chromium navigate to a data: URL).
   Confirm the tool call succeeds and the relevant server process appears **only during** the
   call, inside the sandbox.
3. **Two sandboxes isolated:** two concurrent browser tasks don't share a browser profile
   (each stdio server is `--isolated`).
4. **crack-dev (root) still full-featured:** the host debug endpoints (9930/9931/9932/9877)
   still respond, so a human `pi` session in crack-dev keeps its MCP tools.

## Gotchas

- `.mcp.json` `firefox` writes to `--output-dir /workspace/.playwright-mcp` — inside a
  sandbox that's the overlay (ephemeral, fine). If you want to keep artifacts, repoint it to
  `/crack-harness-data/mcp-out/<conv>` via env; optional.
- npx-based servers download packages on first use; `/root` overlay lower already has the npx
  cache from crack-dev, so first use is warm. Verify no network stall.
- Don't remove the eager block from crack-dev's own entrypoint — the host UI/debugging relies
  on it. Only sandboxes skip it.

## Report

`_slop/report-23/5_mcp_lazy_and_networking.md`: which option (A/B) you took, the measured
at-rest sandbox memory vs the old ~10 GB, browser-tool-on-demand proof, Blender handling
decision, and confirmation crack-dev's host debug endpoints still work.
