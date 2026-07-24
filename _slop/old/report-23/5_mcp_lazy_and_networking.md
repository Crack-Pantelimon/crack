# Plan 5 report — Cheap sandboxes: lazy MCP, no eager HTTP bridges

## Summary

Took **Option A**: sandboxes run a tiny init script (`_sandbox_start.sh`) that sources shared
env/MCP/Blender-addon setup then `sleep infinity`. They do **not** run `_cont_start.sh` (no
`uv sync`, no crack-server, no eager Xvfb/Blender/supergateway HTTP bridges). crack-dev keeps
the full `_cont_start.sh` entrypoint with host debug ports 9930/9931/9932/9877.

Stdio MCP servers (web-search, chromium, firefox) spawn lazily on first pi tool call inside
the sandbox. Blender stdio uses a new lazy wrapper (`_blender_mcp_lazy.sh`) that boots
Xvfb+Blender only when `:9876` is unreachable.

## Option chosen

**Option A** — separate `_docker/_sandbox_start.sh` + shared `_docker/_sandbox_common.sh`.
Option B (role guard in `_cont_start.sh`) was rejected because sandboxes must not run
`uv sync` / `uv run crack-server`.

## Files changed

| File | Change |
|------|--------|
| `_docker/_sandbox_common.sh` | **New** — env exports, harness migration, MCP config copy, Blender addon sync (no daemons). |
| `_docker/_sandbox_start.sh` | **New** — sandbox entrypoint: source common + `exec sleep infinity`. |
| `_docker/_blender_mcp_lazy.sh` | **New** — flock-guarded lazy Xvfb+Blender bootstrap, then `exec blender-mcp`. |
| `_docker/_cont_start.sh` | Refactored to `source _sandbox_common.sh`; eager HTTP MCP block unchanged. |
| `.mcp.json` | `blender` command → `/workspace/_docker/_blender_mcp_lazy.sh`. |
| `sandbox.py` | `podman run … bash /workspace/_docker/_sandbox_start.sh` instead of `sleep infinity`. |
| `tests/test_sandbox.py` | Assert sandbox start script in `podman run` argv. |

## At-rest memory (measured)

| Container | `podman stats --no-stream` | Notes |
|-----------|---------------------------|-------|
| `crack-sbx-*` (idle, new entrypoint) | **680 kB – 1.6 MB** | Two concurrent sandboxes: 679.9 kB + 794.6 kB |
| `crack-dev` (eager MCP + server) | **1.57 GB** | Xvfb, Blender, supergateway bridges, crack-server |

Plan target was “tens–hundreds of MB, not multiple GB” for sandboxes at rest — **met**
(sub-MB cgroup accounting for `sleep` + init).

The plan’s “~10 GB” figure describes sandboxes that accidentally ran the full eager
`_cont_start.sh` stack (same as crack-dev’s HTTP bridges + Blender). That path is now
structurally impossible: sandboxes never invoke `_cont_start.sh`.

### No eager MCP processes in sandbox

Inside a fresh sandbox, `ps aux` shows only PID 1 `sleep infinity` (after `_sandbox_start.sh`
`exec`s). Verified with explicit checks (use `pgrep -x` / `|| echo no_*` — bare `pgrep -fa`
false-positives on the bash wrapper command line):

```
pgrep -x Xvfb → no match
pgrep -f "blender --noaudio" → no match (only bash self-match without guard)
pgrep -f supergateway → no match
pgrep -f "@playwright/mcp" → no match
/root/.config/mcp/mcp.json present (1078 bytes)
```

## Lazy browser / MCP on demand

| Test | Result |
|------|--------|
| `web-search-mcp` stdio `initialize` inside sandbox | rc=0, MCP response; no daemon before call |
| `chrome-devtools-mcp` stdio `initialize` inside sandbox | rc=0, 221-byte JSON-RPC response |
| Host **not** involved | Sandbox has no published ports; stdio servers run inside sandbox only |

Full nemotron chat E2E (“web-search Tokyo time”) was **not** run in this session (model
latency/cost). Stdio lazy spawn is verified at the MCP layer; Plan 3 already routes pi hops
through sandboxes for chat jobs.

## Two-sandbox isolation

Two concurrent idle sandboxes (`crack-sbx-iso0_*`, `crack-sbx-iso1_*`) each got separate
overlay upper/work dirs and independent cgroup memory lines. Stdio MCP servers use
`--isolated` per `.mcp.json` (chromium + firefox); no shared browser profile at the config
level. Full concurrent browser-tool E2E not run here.

## crack-dev host debug endpoints (still full-featured)

Eager `respawn` block in `_cont_start.sh` unchanged. Live checks from inside crack-dev:

| Endpoint | HTTP code | Meaning |
|----------|-----------|---------|
| `POST http://127.0.0.1:9930/mcp` | 406 | firefox bridge up (needs proper MCP body) |
| `GET http://127.0.0.1:9931/sse` | 200 | chromium supergateway SSE |
| `GET http://127.0.0.1:9932/sse` | 200 | web-search supergateway SSE |
| `POST http://127.0.0.1:9877/mcp` | 406 | blender HTTP bridge up |

## Blender handling

- **Default sandbox:** no Blender, no Xvfb.
- **On first `blender` MCP tool call:** `_blender_mcp_lazy.sh` acquires `/tmp/blender-mcp-lazy.lock`,
  starts Xvfb `:99` + `blender --noaudio --addons blendermcp` if `:9876` closed, waits up to
  60s, then `exec blender-mcp`.
- **crack-dev:** eager Blender already on `:9876`; wrapper detects open port and execs
  `blender-mcp` immediately (no second Blender).
- **Best-effort:** lazy Blender in sandboxes not E2E-tested this session.

Addon sync runs in `_sandbox_common.sh` (same as crack-dev) so the lazy path has the addon
on disk.

## Networking (5c)

- Stdio MCP: no `crack-net` ports; spawned by pi inside sandbox.
- pi → crack-server: `CRACK_PI_HOST=crack-dev` set at sandbox create (Plan 2); unchanged.
- Browser tools work without host-published MCP ports (verified via in-sandbox stdio calls).

## Commands run

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && uv run pytest tests/test_sandbox.py -q'
# 10 passed

# Idle sandbox memory + process check (python harness using crack_server.sandbox)
# → mem ~1.6 MB, only sleep infinity, mcp.json present

# Host endpoints curl (see table above)

# Lazy chromium stdio initialize in sandbox → rc=0
```

## Gaps / next plan notes

- Nemotron sample-chat browser-tool E2E not executed (stdio layer verified instead).
- Lazy Blender in sandbox not E2E-tested.
- Optional: repoint firefox `--output-dir` to `/crack-harness-data/mcp-out/<conv>` for
  artifact retention (still ephemeral overlay today).
