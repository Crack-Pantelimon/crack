# Plan 3 report — Route agentic pi hops through sandboxes

## Summary

Agentic hops (`arun_agent_hop`) now run inside per-conversation podman sandboxes when
`sandbox_enabled()` is true (default in crack-dev). One-off `arun_pi_text` calls stay
local in crack-dev. Kill, reload-survival, and destroy lifecycle are wired across the
container boundary.

## I/O model: detached + shared file

**Chosen:** detached `podman exec -d` with pi redirecting stdout to the hop output file
on `/crack-harness-data` (`> /path/to/agent.hop.jsonl 2>&1`).

**Why not piped exec:** piping ties hop I/O to the crack-dev exec client; on reload that
client dies while pi keeps running in the sandbox. The shared-file model makes fresh spawn
and re-attach identical: crack-dev only tails `hop.jsonl` by byte offset from the volume.

## Files changed

| File | Change |
|------|--------|
| `sandbox.py` | `sandbox_enabled()`, sync podman helpers (`session_alive_sync`, `kill_session_sync`, `destroy_sandbox_sync`) |
| `pi_proc.py` | `_HopParams.sandbox`, detached sandbox spawn, sandbox-aware tail/kill/reattach/manifest, `kill_pid_file` reads manifest sandbox+session |
| `chats.py` | `ensure_sandbox` before chat job, pass `sandbox` in hop_kwargs, `destroy_sandbox` on idle/stop/delete |
| `sub_agents/base.py` | `ensure_sandbox(run_id)` per hop, `sandbox=` on `arun_agent_hop`, destroy on stop |
| `sub_agents/runner.py` | `destroy_sandbox_sync(run_id)` in `finish()` |
| `worker.py` | `recover_detached_hops` uses sandbox session liveness |
| `tests/test_sandbox.py` | `sandbox_enabled` tests |

## Kill / reattach rewrite

- **Mid-run stop / time-cap / watchdog:** `sandbox.kill_session(sbx, session_id)` via
  `pkill -f <session_id>` inside the sandbox (TERM → grace → KILL).
- **Explicit STOP / delete:** `kill_pid_file` reads `agent.hop.json` for `sandbox` +
  `session_id`; then `destroy_sandbox(conv_id)` on stop/delete/idle.
- **Reload survival:** manifest stores `sandbox` + `session_id` (no host pid). Re-attach
  tails the shared `hop.jsonl` from stored offset; liveness via `pgrep -f session` in sandbox.
- **Kill switch:** `CRACK_SANDBOX_ENABLED=0` forces local hop path (tests default off via
  non-`/workspace` `CRACK_PI_PROJECT_ROOT`).

## Verification

### 1. Local small calls + unit tests

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. uv run pytest -q tests/'
# 146 passed in 50s
```

`arun_pi_text` / detached-hop tests unchanged (sandbox off in test env).

### 2. Real hop inside sandbox (nemotron HELLO_SANDBOX)

- **Chat id:** `1784719793649`
- **Trajectory:** `/crack-harness-data/unscripted_chats/1784719793649/`
- `crack-sbx-1784719793649` appeared during `chatting`; `pi_in_sandbox=1` observed mid-run
- Overlay: `/crack-harness-data/overlays/1784719793649/upper/HELLO_SANDBOX.txt` → `PONG`
- Host `/workspace/HELLO_SANDBOX.txt` was a stale file from an earlier pre-sandbox run
  (mtime 11:18); this run's write landed only in the overlay (mtime 11:30)

### 3. Mid-run stop

- **Chat id:** `1784719911755`
- `pi=1` inside sandbox before `POST /api/chats/<id>/stop`
- After stop: sandbox destroyed, `phase=idle`

### 4. Reload survival

**Not integration-tested** (would require mid-hop `docker restart crack-dev` against a live
nemotron hop). Implementation mirrors the proven local detached-file model; unit tests for
reattach (`test_detached_hops.py`) still pass. Sandbox reattach uses the same file tail with
`pgrep -f session` liveness instead of host `/proc`.

### 5. Two concurrent chats

- **Chat ids:** `1784719926336` (FILE_A), `1784719926344` (FILE_B)
- Two `crack-sbx-*` containers observed simultaneously (`sandboxes=2`)
- Isolated overlays: `FILE_A.txt` → `A`, `FILE_B.txt` → `B`; both reached `idle`

## Provider / env inside sandboxes

`pi --list-models` works inside a running sandbox (inherits image + `/root` overlay creds).
Dynamic chat env (`CRACK_CHAT_ID`, `CRACK_PARENT_*`, etc.) passed via `podman exec -e` on
each hop spawn. No provider surprises observed for nemotron-120b-super runs.

## Notes for Plan 4+

- `destroy_sandbox` on chat idle means the next message recreates the container (overlay
  upper persists on the volume). Plan 4 patch extraction should run **before** destroy.
- Do not destroy sandbox in a `finally` on `run_chat` — that breaks reload survival when
  the worker job is cancelled mid-hop.
