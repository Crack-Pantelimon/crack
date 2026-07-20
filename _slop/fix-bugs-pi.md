# Fix phantom "pi exited -9": stop SIGKILLing (and mis-crashing) a pi that already finished

## Context

pi workers were reported dying with `-9` (SIGKILL), but the container is **not**
OOMKilled and there is **no kernel OOM**. Verified live in `crack-dev`:

- cgroup v2, `memory.max = max` (no limit); `memory.events` → `oom_kill 0`, `oom 0`.
  memory.peak 9.1 GB against a 128 GB host with 92 GB free. → **OOM fully ruled out.**
- A server reload cannot be the cause: pi is spawned `start_new_session=True` (its
  own session/pgroup), and the reload path *detaches* pi via `CancelledError`
  (`pi_proc.py` `_attempt_once`, `detached=True`) and re-attaches after restart.

**The `-9` is self-inflicted by our own harness.** In `_attempt_once`'s `finally`
([pi_proc.py:757-774](.pi/crack/server/src/crack_server/pi_proc.py#L757-L774)),
after pi's event stream reaches a terminal event (`agent_end`/`sentinel`/`time_cap`),
we wait only **5s** for the pi *process* to exit, then `proc.kill()` (SIGKILL →
returncode `-9`). pi lingers past 5s tearing down its MCP client connections
(chats attach *all* tools: blender + chrome-devtools + playwright + web-search),
so we kill a cleanly-finished pi.

Then it is **misclassified**: on the `agent_end` path `sink.terminated_by_us`
stays `False`, so `_run_hop_with_retries`
([pi_proc.py:924](.pi/crack/server/src/crack_server/pi_proc.py#L924))
computes `failed = not terminated_by_us and rc not in (0, None)` = `True` for
rc `-9`, records `"pi exited -9"`, and **retries a completed hop** — burning the
error budget and masking the true cause.

### Evidence (chats `1784572518962`, `1784569007876`)
- Every one of the 4 recorded errors sits **~5.0–5.8s after** pi's `agent_end`
  timestamp — the exact `proc.wait(timeout=5)` → `proc.kill()` signature.
- The captured `detail` tails show the real trigger that fired this on every hop:
  nvidia provider **hard 429s** — `429 status code (no body)` and
  `ResourceExhausted: Worker local total request limit reached (33/32 … 105/32)`.
  Each hop does one tool call, the next model turn 429s, pi ends the run fast →
  the linger-then-kill path fires back-to-back until the budget is spent.

The 429 rate-limiting is a **separate, known issue** and is out of scope here
(decision: fix the `-9` misdiagnosis only). This plan makes a rate-limited run
report honestly instead of as a phantom SIGKILL.

## Fix

Make the fresh-spawn path (`_attempt_once`) terminal-aware, exactly like the
re-attach path already is (`_reattach_attempt` returns
`returncode = None if sink.terminal else -1` —
[pi_proc.py:850-853](.pi/crack/server/src/crack_server/pi_proc.py#L850-L853)).

**File: `.pi/crack/server/src/crack_server/pi_proc.py`**

1. **`_attempt_once` `finally` block** (~757-774): after a terminal event, never
   SIGKILL — detach and let PID 1 (`podman-init`) reap the lingering pi.
   - Keep a short grace `await asyncio.wait_for(proc.wait(), timeout=EXIT_GRACE_SECONDS)`
     for the common fast-exit.
   - On `TimeoutError`:
     - if `sink.terminal` → **do not kill**. Leave pi running (tini reaps it),
       unlink `pid_file`, set manifest `status="done"`, and report
       `returncode = None` (a terminal stream = clean end).
     - if `not sink.terminal` (a genuinely hung/broken pi with no terminal
       event) → keep today's behavior: `proc.kill()` + `await proc.wait()`,
       manifest `status="crashed"`, rc `-9` retried.
   - `crashed = not sink.terminated_by_us and not sink.terminal and proc.returncode not in (0, None)`.
   - Return dict: `returncode = proc.returncode` (which is `None` when detached
     after a terminal event; the real code when pi exited within grace).
   - Add module constant `EXIT_GRACE_SECONDS` (e.g. 8) near the other tunables;
     replace the literal `5`.

2. **`_run_hop_with_retries` `failed` computation** (~924): harden so terminality
   short-circuits regardless of rc — add `res["terminal"]` from `_attempt_once`
   and `_reattach_attempt`, then
   `failed = not res["terminated_by_us"] and not res["terminal"] and res["returncode"] not in (0, None)`.
   Belt-and-suspenders so a fast terminal exit with any nonzero rc is never a crash.

### Notes / trade-offs
- Leak risk is minimal: `agent_end` already fired, so pi *will* finish teardown
  and exit; PID 1 reaps the zombie. A status-`done` manifest with a briefly-live
  pid is not re-attached (`_live_detached_manifest` only re-attaches
  `status="running"`), and the next hop for the same session spawns fresh — pi
  post-`agent_end` is no longer writing session turns, so no session-dir race in
  practice.
- Do **not** touch the reload-detach path, the watchdog kill
  (`timeout_seconds + 60`), or `kill_pid_file` — those are correct.

## Verification

From `.pi/crack/server/`:

1. **New unit test** (`tests/`, using the `fake_pi.sh` shim): a fake pi that
   emits a valid `agent_end` event then **sleeps longer than `EXIT_GRACE_SECONDS`**
   before exiting. Assert `arun_agent_hop` returns `"agent_end"`, persists its
   turns, and records **no** `"pi exited -9"` error. May require extending
   `fake_pi.sh` with a "linger N seconds after final event" behavior (it already
   supports scripted behaviors like `copy:SRC>DST`).
2. **Regression test**: a fake pi that exits nonzero with **no** terminal event
   still yields `failed=True` and is retried (guards the `not sink.terminal`
   branch).
3. Run: `uv run python -m pytest tests/` (bare `uv run pytest` fails —
   tests import `from tests.test_plan41 import FakePi` and rely on cwd on
   sys.path).
4. **Live sanity** in `crack-dev`: trigger a tools-attached chat; confirm that a
   hop ending in `agent_end` no longer produces a `"pi exited -9"` error card,
   and that the error `at` timestamps no longer trail `agent_end` by ~5s. Any
   remaining failures should now surface the real cause (e.g. the 429) instead
   of a phantom SIGKILL.
