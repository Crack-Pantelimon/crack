# Fix 6 — Delegate retries to pi, surface exact errors, make RPC the default

**Segment 6 of 6. Implement after fix_5 is verified. HIGH RISK — removes old machinery and changes
the default control plane. Do it in the order below and keep the tests green at each step.**

## Goals

1. **pi owns LLM retries.** Transient upstream failures (overloaded / 429 / 5xx) are retried
   *inside* pi via its auto-retry, so the Python worker only sees a run after pi has genuinely
   exhausted its retries. The heavy Python `HARD_RETRY_DELAYS` / no-progress-streak loop shrinks to a
   thin safety net (process died / RPC channel broke).
2. **Exact errors reach the UI.** When pi finally fails, the **real** error string
   (`auto_retry_end.finalError`, a `message_update` error delta, or a `response` `success:false`
   message) becomes the chat's `error` / `error_detail`. No more `pi crashed mid-turn` with a
   useless "No project session found" detail.
3. **Reload survival** without the detached-hop / stdout-tail machinery.
4. **RPC becomes the default** and the dead `--mode json` agent-hop machinery is removed.

## Prereqs
fix_5 landed `crack_server/pi_rpc.py` (`arun_agent_hop_rpc`) behind `CRACK_PI_RPC=1`, and it drives a
single-prompt hop to `agent_settled` with clean `abort`. This segment builds on it.

Read the retry + events sections of the RPC doc first:
```bash
docker exec crack-dev bash -lc 'find / -path "*pi-coding-agent/docs/rpc.md" 2>/dev/null | head -1 | xargs sed -n "380,520p"'
docker exec crack-dev bash -lc 'find / -path "*pi-coding-agent/docs/settings.md" 2>/dev/null | head -1 | xargs sed -n "1,120p"'  # find the `retry` schema
```

## Step 1 — Turn on pi's auto-retry

At the start of every RPC hop (in `arun_agent_hop_rpc`, right after spawn, before the `prompt`),
send:
```json
{"type":"set_auto_retry","enabled":true}
```
Also set a retry ceiling. Prefer pi settings so it's consistent: add a `retry` block to the pi
settings the sandbox uses (check `docs/settings.md` for the exact key names; it is roughly
`{"retry": {"enabled": true, "maxRetries": 5}}`). The chat sandbox reads settings from
`/workspace/.pi/settings.json` and/or `~/.pi/agent/settings.json` — set it where the other crack
defaults live (grep the repo for the existing `settings.json` the harness ships:
`.pi/settings.json`). If settings plumbing is fiddly, the `set_auto_retry` command alone is enough to
enable it; the max attempts can stay at pi's default for now (note it).

Consume the retry events in the hop loop (for observability, not control):
- `auto_retry_start` → log/record a soft note (optional).
- `auto_retry_end` with `success:true` → pi recovered; continue.
- `auto_retry_end` with `success:false` → **genuine failure**: capture `finalError` (the exact
  string) and treat the hop as failed (see Step 2).

## Step 2 — Surface the exact error

The RPC hop must convert pi's real error signals into the existing error plumbing, which already
carries a message + detail all the way to the chat banner:
- `record_error({"message", "detail", "rc", "attempt", "phase"})` — append a durable error row
  (see `steprun.error_recorder`; rows carry `at`, so fix_4 orders them correctly).
- Raise `pi_proc.PiError(message, detail=<exact error>, over_budget=<bool>)` on a terminal failure —
  `steprun.record_chat_errors` already copies `str(e)` → `chat.json.error` and `e.detail` →
  `error_detail`.

Map RPC signals → error text:
| RPC signal | message | detail |
|---|---|---|
| `auto_retry_end` `success:false` | `f"pi gave up: {finalError}"` | `finalError` |
| `message_update` `assistantMessageEvent.type=="error"` (reason `"error"`) | the error text | same |
| `response` `success:false` for our `prompt` | `"pi rejected the prompt"` | the response's message |
| RPC process exited before `agent_settled` (no error event) | `"pi rpc process exited unexpectedly"` | last stderr line(s) captured from the proc's stderr pipe |

Capture the RPC process **stderr** into a small ring buffer (last ~10 lines) so the "process exited"
case has a real detail — but note that with pi owning retries, the common upstream errors now come
through as structured `finalError`, not as a dead process. The old `_compose_detail` "last stderr:"
formatting can be reused for the process-exit fallback.

Confirm the end-to-end path once wired: a forced upstream failure should show, on the chat page, an
error banner whose detail is the **actual** provider error (e.g. `529 overloaded_error: Overloaded`),
not a session warning.

## Step 3 — Shrink the Python retry loop to a safety net

With pi retrying internally, the worker no longer needs `HARD_RETRY_DELAYS` / transient backoff /
no-progress streak logic for the RPC path. In the RPC hop:
- Retry **only** the "RPC channel/process died before any `agent_settled`, and `stop_check` is
  false" case — at most a small fixed number of times (e.g. 2) with a short backoff — because that is
  an infrastructure failure, not an LLM failure.
- Everything pi reports as a genuine failure (Step 2) is surfaced immediately; do **not** loop it.
- Keep the `error_budget` / `over_budget` concept working: if you do record multiple safety-net
  errors, respect `error_budget()` and raise `PiError(over_budget=True)` when spent, so the existing
  "something is likely wrong" banner still appears. (`ratelimit.MAX_TOTAL_ERRORS`,
  `steprun.grant_error_budget` for retry-from-error.)

Do **not** touch `arun_pi_text` (the one-off title/vision `--print --no-tools` calls) — those stay on
the simple print path with their own retry loop.

## Step 4 — Reload survival via the session, not detached tailing

The old json path kept elaborate detached-hop manifests and re-attached to a still-running pi after a
server reload. RPC's process is owned by the worker and dies on reload — that's fine; recover from the
**persisted session** instead:
- On (re)entry to a chat exchange, if a session already exists for `session_id` and the last exchange
  turn is incomplete/absent, **resume the session** by spawning a fresh RPC process on the same
  `--session-dir`/`--session-id` and sending `RESUME_MESSAGE` (from `ratelimit.RESUME_MESSAGE`) as the
  prompt — **never replay the original user message** (that was the duplicate-prompt bug).
- Verify pi RPC resumes an existing session by id/dir: `get_state` after spawn should report the prior
  `messageCount`. If RPC does not auto-continue by id, use the documented session-open flow (see
  `docs/rpc.md` `switch_session` / start options, or keep one RPC process alive per exchange across
  hops so no reopen is needed). Pick whichever the doc supports and note it.

This is a *reduction* in reload machinery and is acceptable: RPC is robust and reloads are rare. It is
fine if an in-flight hop restarts from the last persisted turn after a reload.

## Step 5 — Make RPC the default and remove the dead json machinery

- Flip the default: RPC on unless a kill-switch env (e.g. `CRACK_PI_JSON=1`) forces the old path.
  Update `_docker/run.sh` accordingly (drop the temporary `CRACK_PI_RPC=1`, keep the kill-switch).
- Once RPC is the default and green, delete the now-unused **agent-hop** json machinery in
  `pi_proc.py`: `_attempt_once` sandbox crash-inference, the detached-hop manifest sweep
  (`_sweep_detached_pids`, `_read/_write_hop_manifest`, `_live_detached_manifest`, `_reattach_attempt`),
  the stdout-file tailing (`_tail_events`, `_process_stream_line` if unused), and the
  `_run_hop_with_retries` hard-retry loop — **but keep** anything still used by `arun_pi_text` and by
  `pi_rpc.py` (e.g. `_TurnAccumulator`, `PiError`, `PiStopped`, `kill_pid_file`). Grep every symbol
  before deleting; remove tests that only covered deleted json-agent-hop behavior, and keep/port the
  ones that assert hop *outcomes* (they should pass against the RPC path).

Do this deletion **last**, in its own commit-sized step, so a regression is easy to bisect.

## Step 6 — Error code-path review (the whole path, now that errors are exact)

Walk the path end-to-end and confirm each hand-off carries the real error:
- `pi_rpc` → `record_error` rows (exact message/detail, `at` set) → rendered interleaved (fix_4).
- `pi_rpc` raises `PiError(detail=finalError)` → `steprun.record_chat_errors` → `chat.json.error` /
  `error_detail` / `error_over_budget` → chat banner (`chats.py` renders `error`/`error_detail`).
- `record_chat_errors` still suppresses error text when `stop_requested` (a STOP is not an error) —
  verify (fix_3 kept the guard).
- The retry-from-error button (`grant_error_budget`) still resets the budget and clears the banner.
- Grep the codebase for the literal `"pi crashed mid-turn"` and the "No project session found"
  handling; these should no longer be produced for the RPC path.

## Build / restart
```bash
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh     # ./build.sh only if the Dockerfile changed
```

## Verify

### 1. Unit tests green (including ported hop-outcome tests)
```bash
docker exec crack-dev bash -lc \
  'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
```

### 2. Exact error surfacing (fault injection)
Force a genuine upstream failure and confirm the banner shows the real error. Easiest deterministic
route: point the chat at a model/provider with a bad key or an unreachable endpoint so pi's auto-retry
exhausts and emits `auto_retry_end success:false`, then:
```bash
docker exec crack-dev bash -lc "jq -r '.error, .error_detail' /crack-harness-data/unscripted_chats/$CID/chat.json"
```
**PASS:** `error`/`error_detail` contain the actual provider error text (e.g. an HTTP status +
provider message), not a session warning, and the chat is `idle` (not looping).

### 3. Retries handled by pi, not Python
With auto-retry on, a transient failure should be retried inside pi and the hop should still succeed:
```bash
docker logs crack-dev 2>&1 | grep "$CID" | grep -iE "auto_retry|gave up|retry" | head
```
**PASS:** you see pi's `auto_retry` activity (or none), and you do **not** see the Python worker
spawning multiple fresh pi processes for one hop (one RPC process per hop; count `podman exec` for the
chat stays ~1 per prompt).

### 4. Reload survival
Start a longer chat, restart the server mid-run (`cd _docker && ./run.sh`), and confirm the chat
resumes from its last persisted turn using `RESUME_MESSAGE` — **not** by re-sending the original user
prompt (check the session file / trajectory does not contain a duplicated original prompt).

### 5. Full regression against the original cascade
Re-run the fix_1/fix_2/fix_3 live checks with RPC as default: single-file chat → clean patch, applies,
no `patch_apply` loop; STOP sticks; errors interleave in order; exactly one session file per chat.

## Done when
pi owns LLM retries, genuine failures show the exact provider error in the chat banner, RPC is the
default with the json agent-hop machinery removed (except the `arun_pi_text` one-off path), reloads
resume from the last turn without replaying prompts, and all earlier segments' checks still pass.
