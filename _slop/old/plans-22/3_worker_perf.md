# Plan 3 — Worker & async hygiene (cheap fixes; keep the worker in-process)

**Decision (locked):** **no out-of-process RPC rewrite.** The primary cause of stalling under many
sub-agents is resource saturation from too many concurrent `pi` subprocesses — addressed by the
`MAX_PARALLEL_SUBAGENTS = 3` cap in Plan 2. This plan removes the remaining event-loop-blocking IO and adds
an in-flight dispatch cap as a safety net. A separate worker process would not fix subprocess saturation
and would add serialization, a private port, lifecycle, and re-attach complexity for no benefit here.

**Scope:** `.pi/crack/server/src/crack_server/`: `worker.py`, `state.py`, `queue.py`,
`sub_agents/base.py`, `sub_agents/runner.py`, `context_stats.py`, `pi_proc.py`.
**Tests:** `python -m pytest -q` from the server dir. Relevant: `test_async_worker.py`, `test_state.py`.

**Facts established during inspection (so a fresh context trusts them):**
- The worker already runs **in-process** as an asyncio task ([worker.py:242](../../.pi/crack/server/src/crack_server/worker.py) `async_loop`), one task per job, **no cap** (worker.py:243 docstring / 266).
- The pi subprocess layer is genuinely async (`asyncio.create_subprocess_exec`, tail-poll via
  `asyncio.sleep`) — pi_proc.py. Do **not** rewrite that.
- Blocking work that currently runs **on the event loop**:
  - `queue.claim_next()` — synchronous `glob` + `read_text` of pending/ — called every loop iteration
    (worker.py:263).
  - Post-dispatch bookkeeping in `_dispatch` (worker.py:61-73): `queue.complete`, `paths.*.read/update`,
    `persona.enqueue_step` (which calls `queue.enqueue_exclusive` → `_find_job` globs **both** dirs and
    reads every job file).
  - The whole sub-agent dispatch chain (`base.py._run_hop` / `runner.finish`) does synchronous
    `JsonState.update` under an exclusive **`fcntl.flock`** ([state.py:60-76](../../.pi/crack/server/src/crack_server/state.py)). A contended flock **blocks the loop thread**, stalling every coroutine (including HTTP long-polls).
  - `context_stats.render_context_line` reads a 512 KB session tail **per run-card per 2 s poll**
    (context_stats.py `_read_tail_lines`). The sub-agent card/sidebar routes are sync `def` (threadpool),
    so this doesn't block the loop, but with N cards × frequent polls it burns threadpool threads and disk.

Do the sub-tasks in order; each is independently shippable.

---

## Task 3.1 — Cap concurrent in-flight dispatch tasks (safety net)

**Problem.** `async_loop` claims and launches an asyncio task per pending job with no cap
(worker.py:262-266). Even with Plan 2's spawn cap, retries/resumes/model-refresh can pile up; nothing
bounds how many `pi` hops run at once.

**Fix.** Add a module-level `asyncio.Semaphore` sized to a small constant and acquire it around the actual
hop dispatch. Recommended sizing: reuse `MAX_PARALLEL_SUBAGENTS` (3) plus a small headroom for the chat
root and models refresh, e.g. `WORKER_MAX_INFLIGHT = MAX_PARALLEL_SUBAGENTS + 2`. Implementation:
```python
WORKER_MAX_INFLIGHT = 5   # keep aligned with MAX_PARALLEL_SUBAGENTS (+ chat + models refresh)
_SEM: asyncio.Semaphore | None = None   # created in async_loop

async def _dispatch(job):
    async with _SEM:
        ... existing body ...
```
Keep claiming jobs into `in_flight` as today, but because claimed jobs move to `processing/`, a job that
can't get the semaphore simply waits inside its task — fine. **Watch out:** don't claim so many that
`processing/` fills unboundedly; optionally stop claiming when `len(in_flight) >= WORKER_MAX_INFLIGHT * 2`
(leave a claim backlog in `pending/`). Add that guard in the claim loop:
```python
while len(in_flight) < WORKER_MAX_INFLIGHT * 2:
    job = await asyncio.to_thread(queue.claim_next)   # see Task 3.2
    if job is None: break
    in_flight.add(asyncio.create_task(_dispatch(job)))
```

**Verify:** `test_async_worker.py` — enqueue many jobs backed by a slow `fake_pi`, assert no more than
`WORKER_MAX_INFLIGHT` hops run concurrently (e.g. the fake counts concurrent invocations via a lock file /
counter and asserts the peak). `python -m pytest -q`.

---

## Task 3.2 — Move queue scans off the event loop

**Fix.**
- In `async_loop`, call `queue.claim_next()` via `await asyncio.to_thread(queue.claim_next)` (shown above).
- In `_dispatch`, the post-completion block (worker.py:61-73) runs `queue.complete`, state reads/updates,
  and `persona.enqueue_step`. Wrap the enqueue/`enqueue_exclusive` + the `paths.chat_state(...).read()` /
  `.update()` calls that happen here in `asyncio.to_thread(...)`. Simplest: extract the post-dispatch
  bookkeeping into a small sync function `_finalize_dispatch(job, slug, task_id, persona, run_id, successor)`
  and call it with `await asyncio.to_thread(_finalize_dispatch, ...)`.
- `queue.enqueue_exclusive`'s `_find_job` is O(jobs). Low-effort improvement: in `_find_job`, skip
  `.tmp` files and stop at the first match (already does). That's enough given the in-flight cap keeps the
  queue small. Do **not** redesign the queue.

**Verify:** existing queue tests still pass; add an assertion that `async_loop`'s claim path uses a thread
(hard to unit-test directly — a grep gate `grep -q "to_thread(queue.claim_next" worker.py` plus the
async-worker throughput test suffices). `python -m pytest -q`.

---

## Task 3.3 — Off-loop / awaitable JSON state updates in the async dispatch chain

**Problem (the real loop-staller).** `JsonState.update` takes an exclusive `flock`; when contended
(many sub-agents + threadpool poll routes hitting the same lock files) the loop thread blocks.

**Fix.** Add async twins in `state.py` that run the blocking body in a thread:
```python
async def aupdate(self, fn):
    import asyncio
    return await asyncio.to_thread(self.update, fn)

async def aread(self):
    import asyncio
    return await asyncio.to_thread(self.read)
```
Then switch the **hottest async-context call sites** to `await ...aupdate(...)` / `aread`:
- `base.py`: inside `_run_hop`, `_after_hop`, `dispatch_step`, `_begin_run` — the `self.state_update` /
  `self.state_read` calls that run between `await`s. Provide `async def astate_update`/`astate_read`
  wrappers on `SubAgentPersona` delegating to the new `JsonState` async methods, and use them in the async
  methods. Leave the sync `state_update`/`state_read` for the sync callers (routes in threadpool, retry).
- `runner.finish` is sync but is invoked from async dispatch; it does several `flock` updates. Either make
  an `afinish` used from the async paths, or keep `finish` sync but ensure it's only ever called via
  `await asyncio.to_thread(runner.finish, ...)` from the loop. **Pick:** wrap the `runner.finish(...)`
  calls that execute inside async methods (base.py, runner.py) with `to_thread`. Grep for `runner.finish(`
  and audit each: if the enclosing function is `async`, route through `to_thread`.

This is mechanical but touch-heavy. Keep changes minimal and behavior-identical (same fn, just off-loop).

**Verify:** `test_state.py` — add a test that `JsonState.aupdate` produces the same result as `update`
under a temp file, and that two concurrent `aupdate`s serialize correctly (final value reflects both).
`test_sub_agents.py` must still pass unchanged (behavior identical). `python -m pytest -q`.

---

## Task 3.4 — Memoize session-usage reads (throttle the 512 KB tail)

**Problem.** `context_stats.session_usage` (+ the new estimator from Plan 1.5) reads up to 512 KB per run
per poll. With many cards polling every 2 s this is heavy disk churn in the threadpool.

**Fix.** Add a tiny process-local cache keyed by `(session_path, mtime)`:
```python
_USAGE_CACHE: dict[str, tuple[float, dict | None]] = {}   # path -> (mtime, result)
```
In `session_usage`, after `_newest_session`, `stat` the file; if `_USAGE_CACHE[path]` has the same mtime,
return the cached result; else compute, store, return. Bound the cache (e.g. drop entries when it exceeds
~256 keys, or key-evict on read). This makes repeated polls of an *unchanged* session O(1). Same for the
Plan-1.5 estimator (share the cache/key). No behavior change when the file changes (mtime differs → recompute).

**Verify:** unit — call `session_usage(dir)` twice without modifying the file; assert the second call does
not re-read (patch `_read_tail_lines` with a call counter, assert it's called once). Then touch/append the
file and assert it recomputes. `python -m pytest -q`.

---

## Task 3.5 — Audit remaining on-loop blocking (review, fix only clear wins)

Quick review pass; fix anything obviously blocking, don't gold-plate:
- `pi_proc.py` `_kill_process_group` uses a synchronous `time.sleep(0.1)` (~line 485). Confirm its callers:
  if reachable from an `async` path on the loop, replace the sleep-loop with `await asyncio.sleep` in an
  async variant, or call the killer via `to_thread`. If it's only reached from signal/stop handlers off
  the loop, leave it and note so.
- Async routes that long-poll (`chat_wait`, `api_chat_dots_wait`, `api_wait_sub_agents`): confirm their
  per-iteration work is cheap (small state reads) and they `await` between checks (they do — event-based).
  If any does a heavy synchronous scan per wake, wrap it in `to_thread`.
- `worker._prune_old_session_dirs` / `recover_detached_hops` are already `to_thread`'d at startup
  (worker.py:253-255) — leave them.

**Verify:** `python -m pytest -q` (no regressions). Document findings inline as code comments where a spot
was reviewed and deemed safe, so the next reader doesn't re-audit.

---

## Plan 3 completion checklist (automatic)
From `.pi/crack/server`:
```
python -m pytest -q
grep -q "to_thread(queue.claim_next" src/crack_server/worker.py
grep -q "WORKER_MAX_INFLIGHT" src/crack_server/worker.py
grep -q "async def aupdate" src/crack_server/state.py
grep -q "_USAGE_CACHE" src/crack_server/context_stats.py
```
**Load test (manual, the real acceptance):** with Plan 2 merged, run a chat that fans out to the parallel
cap (3 sub-agents) plus queued extras; keep the chat page open. Confirm: page/tree polls stay responsive
(no multi-second stalls), the worker log shows in-flight bounded by `WORKER_MAX_INFLIGHT`, and completing
one sub-agent promptly starts the next. Compare against the pre-change behavior (15 concurrent procs) to
confirm the stall is gone.


Use all uv, python, bash, etc. commands only under the following way: docker exec crack-dev bash -exc 'command .... ' . Do not run any code outside the container, you will not have the tools available at all. You can use rg, fzf, grep, inside the container only. You can use edit and read commands as normal. The container mounts the workspace dir at /workspace where your shells will spawn. 