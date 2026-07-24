# Plan 3 — Route agentic pi hops through sandboxes (kill + reload-survival)

> Read `0_overview.md`; requires Plans 1 & 2. This is the biggest, riskiest plan: it makes
> every real agent hop run **inside a sandbox** via `podman exec`, and rewires kill +
> reload-survival to work across the container boundary. Go carefully; keep crack-dev bootable.

## What changes

`pi_proc.py` currently spawns pi **locally** in crack-dev (`asyncio.create_subprocess_exec("pi", ...)`)
and manages it with host pids (`os.killpg`, `/proc/<pid>/cmdline`). For sandboxed
conversations, pi must run **inside** `crack-sbx-<conv_id>` via `podman exec`, and the host
pid is useless (verified: killing the exec client does not kill pi inside).

**Scope boundary:** only the *agentic* hops move — `arun_agent_hop` (the streaming JSON hop
in `pi_proc.py`). The small `arun_pi_text` calls (title/summary/vision, `--no-tools`) **stay
local in crack-dev** (they don't touch the tree and must keep working while sandboxes churn).

## Design

### 3a. Sandbox-aware command build

Add a `sandbox: str | None` field to `_HopParams`. When set, `_build_cmd` wraps the pi argv:

```python
def _wrap_for_sandbox(p, cmd: list[str]) -> list[str]:
    if p.sandbox is None:
        return cmd
    env_args = []
    for k, v in (p.env_extra or {}).items():
        env_args += ["-e", f"{k}={v}"]
    # pi's cwd inside the sandbox is the overlaid /workspace; extension + SYSTEM.md
    # paths are all under /workspace so they resolve identically.
    return ["podman", "exec", "-w", "/workspace",
            "-e", "CRACK_HARNESS_DATA_DIR=/crack-harness-data",
            "-e", "CRACK_PI_HOST=crack-dev",
            *env_args, p.sandbox, *cmd]
```

The `pi` argv is otherwise unchanged (`--session-id`, `--session-dir`, `-e <ext>`,
`--append-system-prompt`, `--mode json`, ...). **Crucially, `--session-dir` already points
under `/crack-harness-data` (Plan 1), a shared mount** — so session files pi writes inside
the sandbox are visible to crack-dev, and resume across hops works within the conversation.

### 3b. Hop I/O on the shared mount (reload survival)

Today `_attempt_once` opens the hop output file locally and redirects pi's stdout to it.
Keep that pattern but ensure the file lives on `/crack-harness-data` (it does, via Plan 1's
pid/hop path helpers — verify `hop_output_path`/`hop_manifest_path` derive from the run's
pid_file which is under `unscripted_chats` → now on the volume). Then:

- Run pi **detached inside the sandbox** writing to that shared file, OR keep piping stdout
  through `podman exec` (no `-d`) into the locally-opened file. **Prefer piping** for the
  first cut: `podman exec` (no `-t`, no `-d`) forwards pi's stdout to the fd crack-dev opened,
  so the existing `_tail_events` file loop is unchanged. Reload survival: on a crack-dev
  reload the exec client dies and its stdout pipe closes — **but pi keeps running in the
  sandbox** (separate container). The restarted worker's `_reattach_attempt` must therefore
  re-tail by re-execing a reader against the sandbox rather than trusting the closed pipe.

  Concretely, change the reattach path: instead of reading a local file the dead pi was
  writing, on reattach do `podman exec <sbx> tail -c +<offset> -f <hop_file_in_sandbox>`?
  Simpler and robust: **make pi write the hop file itself** to a shared path and run the exec
  detached (`podman exec -d ... pi ... > /crack-harness-data/.../hop.jsonl`), then crack-dev
  only ever *tails the shared file* (fresh spawn and reattach identical). This deletes the
  pipe-vs-reload ambiguity. Recommended: adopt the detached+shared-file model here.

- `pid_file`: still written, but store the **in-sandbox pid** of pi (capture it from a tiny
  wrapper: `podman exec -d <sbx> sh -c 'echo $$ > <pidfile>; exec pi ...'`) OR skip pid files
  entirely and identify pi by `--session-id` in its cmdline (kill uses `pkill -f <session>`).
  Recommended: **drop host pid reliance**; keep a `<sbx>` + `<session_id>` tuple.

### 3c. Kill path rewrite

Replace the host-pid machinery for sandboxed hops:

- Mid-run stop / time-cap / watchdog → `sandbox.kill_session(sbx, session_id)` =
  `podman exec <sbx> pkill -TERM -f <session_id>`, escalate to `-KILL` after a grace.
- Full conversation stop / delete → `sandbox.destroy_sandbox(conv_id)` (`podman kill` + `rm -f`).
- Keep `arun_pi_text`'s local `kill_pid_file` for the non-sandboxed small calls.

Gate all of this on `p.sandbox is not None` so local behavior is untouched when sandboxing
is off (keep a `CRACK_SANDBOX_ENABLED` env flag defaulting on in-container, off in tests).

### 3d. Wire the callers

- `sub_agents/base.py` / `runner.py` and the chat worker path (`chats.py` / `worker.py`):
  before the first hop of a conversation, `await sandbox.ensure_sandbox(conv_id)`; pass the
  sandbox name into `arun_agent_hop(sandbox=...)`.
- On terminal finish/stop of a conversation, `await sandbox.destroy_sandbox(conv_id)` (Plan 4
  extracts the patch *before* destroy).
- `conv_id` = chat_id for a top-level chat, run_id for a sub-agent run.

## Verification

1. **Local small calls unaffected:** run the title/summary path (any existing test that
   exercises `arun_pi_text`), plus `uv run pytest -q tests/` — keep green (add a
   `CRACK_SANDBOX_ENABLED=0` default in the test env if needed).
2. **A real hop runs inside a sandbox.** Drive the nemotron sample chat (task:
   "Create `/workspace/HELLO_SANDBOX.txt` with the word PONG, then stop."). While it runs:
   ```bash
   docker exec crack-dev bash -exc 'podman ps --format "{{.Names}}" | grep crack-sbx'
   # prove pi is INSIDE the sandbox, not crack-dev:
   docker exec crack-dev bash -exc 'podman exec $(podman ps --format "{{.Names}}"|grep crack-sbx|head -1) bash -c "ls -la /proc/*/cmdline >/dev/null; ps -ef 2>/dev/null | grep -c [p]i || true"'
   ```
   After completion, confirm the file exists **in the overlay upper** (on the volume) but the
   host `/workspace/HELLO_SANDBOX.txt` does **not** exist:
   ```bash
   docker exec crack-dev bash -exc 'ls /crack-harness-data/overlays/*/upper/HELLO_SANDBOX.txt; ls /workspace/HELLO_SANDBOX.txt 2>&1 | tail -1'
   ```
3. **Mid-run stop kills pi in the sandbox:** start a longer task, POST
   `/api/chats/<id>/stop`, then verify no `pi` process remains in the sandbox and the chat
   goes to a stopped/idle phase.
4. **Reload survival:** start a multi-turn task; mid-hop run `./run.sh` (or
   `docker restart crack-dev`); confirm the worker re-attaches and the chat still completes
   (turns not duplicated). Inspect `hop.json` status transitions on the volume.
5. **Two concurrent chats don't collide:** run two nemotron chats at once, each writing a
   different file; confirm two `crack-sbx-*` containers and two isolated overlays.

## Gotchas / risks

- `podman exec` env: pi needs the same env crack-dev set (PATH, model provider keys, MCP
  config). Those come from the sandbox image + `/root` overlay; pass any dynamic ones with
  `-e`. Verify `pi --list-models` works inside a sandbox (provider creds resolve).
- Don't SIGKILL pi on the terminal-event grace path across the boundary — mirror pi_proc's
  "linger on MCP teardown" logic but express it as `podman exec ... pkill` timing.
- If you adopt detached+shared-file (recommended), rewrite `_reattach_attempt` to tail the
  shared file by offset (it already does) and check liveness via
  `podman exec <sbx> pgrep -f <session>` instead of `/proc`.
- Keep a kill-switch: `CRACK_SANDBOX_ENABLED=0` must fully restore old local behavior so you
  can bisect regressions.

## Report

`_slop/report-23/3_route_hops_through_sandboxes.md`: which I/O model you chose (piped vs
detached+shared-file) and why, the kill/reattach rewrite, all five verification results
(with the sample chat ids), and any provider-cred/env surprises inside sandboxes.
