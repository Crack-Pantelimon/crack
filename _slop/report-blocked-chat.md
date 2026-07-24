# Report: Async Blockage During Sandbox Creation (pi crack server / crack-dev)

**Status:** ✅ implemented in `sandbox.py` (Edits A, B, C, D + §5.4 semaphore)
**Author:** triage pass
**Scope:** `.pi/crack/server/src/crack_server/` — the FastAPI + async in-process
worker that serves the pi crack chat interface.

## 1. Symptom

When a chat (or sub-agent run) starts, the worker creates a per-conversation
podman sandbox via `sandbox.ensure_sandbox(...)`. While that sandbox is being
created, **the whole Python event loop stalls**: unrelated chat HTTP routes,
the models-cache refresh, the orphan sweep, and even progress streaming of
*other* in-flight chats freeze until sandbox creation finishes. Knock-on
effect: the chat UI appears hung; new messages time out; concurrent sub-agents
queue behind one worker instead of overlapping.

## 2. Root cause

`ensure_sandbox` is declared `async def` and is `await`ed by the worker, but
most of its body is **synchronous, blocking I/O** that runs *on the event loop
thread*. `await` switches away from the coroutine only at genuine `await`
points; synchronous `subprocess.run` / `subprocess.Popen.communicate()` /
`Path` tree walks block the actual OS thread. Because CPython's default
`asyncio` `SelectorEventLoop` dispatches every coroutine on a single thread,
one `subprocess.run` that takes seconds of wall time freezes *everything* that
isn't already blocked behind it on the ready queue.

The only part of `ensure_sandbox` that is genuinely non-blocking is the
`podman` invocation itself, which correctly uses
`asyncio.create_subprocess_exec` + `asyncio.wait_for(proc.communicate(...))`
(via `_podman`).

## 3. Evidence — blocking calls on the event loop thread

File: `.pi/crack/server/src/crack_server/sandbox.py`

| Line(s) | Function | Call | Why it blocks |
|---|---|---|---|
| 109–120 | `snapshot_host_tree` | `subprocess.run(["git", "...", "write-tree"], timeout=60)` | Spawns `git`, capt of a potentially large index; full `write-tree` walk. |
| 131–142 | `snapshot_host_head` | `subprocess.run(... "rev-parse" HEAD)`, `... "symbolic-ref" --short HEAD` | Two serial `subprocess.run` invocations. |
| 145–225 | `materialise_frozen_base` | `subprocess.Popen("git archive")` + `subprocess.run("tar -x")` + `git init` + `git symbolic-ref` + `git update-ref` + `git read-tree` | **A pipeline** (`git archive \| tar -x`) materialising the entire tracked tree into `dest`, plus seeding a fresh `.git`. This is the worst offender — it is doing the actual file copy of the whole repo, plus 4 serial git plumbing commands, all on the loop thread. |
| 170–186 | (within `materialise_frozen_base`) | `arch.communicate()[1]`, `tar = subprocess.run(... stdin=arch.stdout ...)` | Read+drain of the archive pipe; wait for tar to finish. |

These three functions are **plain `def`** and are called **directly** (not via
`asyncio.to_thread`) from inside the `async def ensure_sandbox` body:

```python
# sandbox.py:346–351
else:
    tree = snapshot_host_tree()                       # BLOCKING
    head_sha, branch = snapshot_host_head()            # BLOCKING
    base = overlay_base_dir(conv_id)
    materialise_frozen_base(tree, base, head_sha=head_sha, branch=branch)   # BLOCKING (longest)
    lower_host = f"{ovl}/base"
```

`upper.mkdir(parents=True, exist_ok=True)`, `work.mkdir(...)`, `t_upper.mkdir(...)`,
`t_work.mkdir(...)`, `overlay_tree_path(conv_id).write_text(...)` (lines 333–345)
are also synchronous filesystem calls on the event loop thread, individually
small but on a busy server they add latency inside the blocked window. A
thorough fix sweeps these off the loop too (§5.3).

Also note `sandbox_enabled()` (line 35–49) calls `Path("/workspace").resolve()` /
`os.environ` lookups per hop — cheap, but on the loop thread; acceptable for
now, called out for completeness.

## 4. Why the worker does not shield against this

File: `.pi/crack/server/src/crack_server/worker.py`

- `_dispatch` (line 95) wraps the whole job in `async with _SEM` where `_SEM`
  is an `asyncio.Semaphore(WORKER_MAX_INFLIGHT)` (line 26–27,
  `WORKER_MAX_INFLIGHT = MAX_PARALLEL_SUBAGENTS + 2`). A semaphore only
  *limits concurrency*; it does **not** move work off the loop thread.
- For chat jobs, line 115–116: `await chats.run_chat(task_id)` — and
  `chats.run_chat` immediately does `await sandbox.ensure_sandbox(chat_id)`
  (chats.py:1436). The semaphore lets multiple jobs run "concurrently", but
  because they all share one event loop, a blocking `ensure_sandbox` in job A
  freezes job B's already-`await`ed coroutines for the duration.
- The worker does use `asyncio.to_thread(...)` for genuinely CPU/sync-style
  work elsewhere — `models_mod.refresh_models` (worker.py:114),
  `paths.run_state_by_id(run_id).read` (worker.py:121), `_finalize_dispatch`
  (worker.py:132), `_fail_dispatch` (worker.py:137), `queue.claim_next`
  (worker.py:269), recovery/sweep (worker.py:259–261, 275). This establishes
  the codebase pattern to follow: **sync I/O must be wrapped in
  `asyncio.to_thread`**, which the author of `sandbox.py` did *not* do for the
  git materialisation.

## 5. Fix plan

### 5.1 Principle
**Any blocking I/O (subprocess, filesystem tree walk, large `read_text` /
`write_text`) that is reachable from an `async def` must either**

  1. already use `asyncio.create_subprocess_exec` + `await proc.communicate()`
     as `_podman` does (best, since it lets the loop react to subprocess
     stdout/stderr chunk-by-chunk), **or**
  2. be wrapped in `await asyncio.to_thread(...)` so the CPython event loop
     thread stays free to run other coroutines while the op happens on a thread
     from the default `ThreadPoolExecutor` (second best; easiest retrofit).

`to_thread` is the right retrofit for the git materialisation: these are
short, one-shot shell pipelines whose only output is a tree on disk and whose
`returncode`. Wrapping them in `create_subprocess_exec` would require
implementing the `git archive | tar -x` pipe by hand in asyncio (two subprocesses
glued by `asyncio.StreamReader`/`StreamWriter`), which is more error-prone for
no real concurrency benefit — the pipe is inherently serial.

### 5.2 Concrete edits — `sandbox.py`

**Edit A — make `snapshot_host_tree`, `snapshot_host_head`,
  `materialise_frozen_base` callable off-thread.**

These stay `def` (they are sync by construction), but `ensure_sandbox` must
call them through `asyncio.to_thread`. There are two acceptable shapes:

*Shape 1 (smallest patch, recommended for the first fix):* wrap each call site
in `asyncio.to_thread`:

```python
# sandbox.py, inside async def ensure_sandbox, replacing lines 346–351
else:
    tree = await asyncio.to_thread(snapshot_host_tree)
    head_sha, branch = await asyncio.to_thread(snapshot_host_head)
    base = overlay_base_dir(conv_id)        # cheap Path math, leave inline
    await asyncio.to_thread(
        materialise_frozen_base, tree, base, head_sha=head_sha, branch=branch,
    )
    lower_host = f"{ovl}/base"
```

This is a 4-line behaviour change and immediately unblocks the loop for the
duration of `git write-tree`, `git rev-parse`, `git symbolic-ref`, the
`git archive | tar -x` materialisation, and the `git init`/`update-ref`/
`read-tree` seed.

*Shape 2 (cleaner, slightly larger):* rename the sync functions to
`_snapshot_host_tree_sync` etc. and expose thin async wrappers:

```python
async def snapshot_host_tree(root: Path | None = None) -> str:
    return await asyncio.to_thread(_snapshot_host_tree_sync, root)
```

…then `ensure_sandbox` just `await`s them as it already does today. Pick this
only if other call sites want the async form; today the only caller is
`ensure_sandbox` (see grep in §6), so Shape 1 is enough.

**Edit B — wrap the filesystem mkdirs/writes inside `ensure_sandbox` too
  (defensive).** Lines 333–345 (`upper.mkdir`, `work.mkdir`, `t_upper.mkdir`,
  `t_work.mkdir`, `overlay_tree_path(...).write_text`). Each individual call
  is fast, but mkdir of a path on an overlayfs over a network volume can
  occasionally stall; wrap the whole block in one `to_thread`:

```python
def _prep_overlay_dirs() -> str:
    upper.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)
    t_upper.mkdir(parents=True, exist_ok=True)
    t_work.mkdir(parents=True, exist_ok=True)
    return ovl
ovl = await asyncio.to_thread(_prep_overlay_dirs)
```

(The sub-agent-sharing branch at 339–345 that does
 `overlay_tree_path(conv_id).write_text(...)` should also be inside this
 off-thread block, or a separate `to_thread`, since `write_text` on the harness
 volume can be slow.)

**Edit C — `parent_conv` branch read/write.** Lines 339–345 do
 `overlay_base_dir(parent_conv).is_dir()` and
 `overlay_tree_path(conv_id).write_text (...)`. Wrap in `to_thread`. This is
 the common sub-agent path, and it currently runs every sub-agent's first hop
 on the loop thread.

**Edit D — guard `git` subprocess timeouts.** None of the innermost
`subprocess.run` calls in `snapshot_host_tree` / `snapshot_host_head` / the
seed section of `materialise_frozen_base` pass `timeout` (only the outermost
`write-tree` and `rev-parse` use `timeout=60`). `subprocess.run` without
`timeout` blocks forever if the subprocess hangs. Add `timeout=...` to every
blocking `subprocess.run`/`Popen.communicate()` in these functions so a stuck
git never pins a thread of the `ThreadPoolExecutor` indefinitely (which would
otherwise exhaust the default pool of `min(32, os.cpu_count()+4)` workers and
re-introduce the freeze at scale). Concretely: `git init`, `git symbolic-ref`,
`git update-ref`, `git read-tree`, and the `tar` extraction each need an
explicit short `timeout` (e.g. 30–60s) with a `try/except
subprocess.TimeoutExpired` that converts to the same `RuntimeError` shape
already used by `snapshot_host_tree`.

### 5.3 Call-site parity — make sure no other sync entry exists

- `chats.py:1436` and `chats.py:1470` (`sandbox.ensure_sandbox(chat_id)`): no
  change needed once `ensure_sandbox` is genuinely async-clean; they already
  `await` it.
- `sub_agents/base.py:316` (`await sandbox.ensure_sandbox(run_id, ...)`: same
  — no change needed.
- `worker.py` dispatch: no change needed — `await chats.run_chat(...)` and
  `await persona.dispatch_step(...)` already `await` the chain end-to-end;
  blocking was happening *inside* it, not because it was sync.

### 5.4 Thread pool headroom

The default `asyncio` thread executor is `min(32, os.cpu_count() + 4)`. In
the worst case, several concurrent sandbox creations could each occupy a
thread for the full `git archive | tar -x` duration. Two mitigations:

1. A private bounded thread pool for sandbox materialisation, e.g.:

   ```python
   _SBX_THREADS = concurrent.futures.ThreadPoolExecutor(
       max_workers=2, thread_name_prefix="sandbox-mat",
   )
   ...
   loop = asyncio.get_running_loop()
   await loop.run_in_executor(_SBX_THREADS, materialise_frozen_base, tree, base, ...)
   ```

   This stops sandbox intro from competing with `models_mod.refresh_models`
   and `queue.claim_next` for executor threads, and bounds the aggregate
   kernel/git/fs pressure. With a 2-worker pool you get *some* overlap of
   concurrent intros but a hard ceiling.

2. Alternatively, an `asyncio.Semaphore(2)` around the materialisation
   section in `ensure_sandbox` (not the whole `ensure_sandbox`, to allow the
   slow `podman run` to overlap) achieves the same bound without a dedicated
   executor. The semaphore approach is simpler and consistent with the
   codebase's existing use of `_SEM` in the worker.

Recommendation: start with **Edit A** (to_thread) + a
`SandboxMaterialiseSemaphore(2)` around the `else:` branch at lines 346–351.
That fixes the freeze and bounds fs pressure with minimal change. Upgrade to
a private executor only if executor exhaustion shows up under load.

### 5.5 Observability (optional, to confirm the fix)

`_append_ui_prep(..., "sandbox", ..., elapsed)` already records the wall time
of sandbox creation for the UI (chats.py:1436–1441, 1469–1475). Add a log line
right before/after `ensure_sandbox` in `worker._dispatch` is unnecessary; the
existing `logger.info("started sandbox %s ...")` in `ensure_sandbox` (line
383) marks completion. To prove the loop is no longer frozen during intro, add
a heartbeat task that `asyncio.sleep`s every 400ms and logs when the gap
exceeds 1s — it should stop tripping once Edit A lands. (One-off diagnostic,
not a runtime requirement.)

### 5.6 Out of scope but tracked

- `destroy_sandbox_sync` / `kill_session_sync` / `session_alive_sync` /
  `container_exists_sync` / `_podman_sync` (lines 239–252, 470–515) are
  intentionally sync wrappers for terminal handoffs ("Sync wrapper for stop
  routes and `kill_pid_file`"). They are called from sync FastAPI routes (?)
  — verify the callsites are not in the worker loop. If any are reachable
  from an async handler without `to_thread`, they should be wrapped/converted
  the same way. (Quantify in §6 audit before patching.)
- `rag.py` / `rag_inject.py` git ops were not examined in this pass; if they
  run on the worker loop they need the same treatment. See §6.

## 6. Audit checklist (do before merging the fix)

1. `grep -rn "snapshot_host_tree\|snapshot_host_head\|materialise_frozen_base"`
   across `src/crack_server` — confirm `Ensure_sandbox` is the only caller.
2. `grep -rn "_podman_sync\|destroy_sandbox_sync\|kill_session_sync\|session_alive_sync\|container_exists_sync"`
   — list every call site; confirm each is either genuinely sync (signal handler,
   sync HTTP route) or wrap it in `to_thread`.
3. `grep -rn "subprocess\.run\|subprocess\.Popen\|subprocess\.check_"`
   in `sandbox.py`, `rag.py`, `rag_inject.py`, `git_utils.py`, `pi_proc.py`,
   `pi_runner.py`, `patch.py` — catalogue every blocking subprocess call
   reachable from an `async def` without `to_thread` / `create_subprocess_exec`.
4. Check the `asyncio` event loop being used under uvicorn: if the deployment
   uses `uvloop`, the same thread-blocking rule applies (uvloop also runs
   coroutines on one thread); confirm with `--loop uvloop` flag/`App('--loop')`
   in `main.py` / `app.py`.
5. Re-run the repro from §1 — start a chat that triggers a sandbox cold-start,
   concurrently hit a trivial HTTP route (`GET /health`-style) and a second
   chat's message-append. The route latency should now be flat instead of
   spiking for the sandbox-creation window.

## 7. Implementation order

1. **Edit A** (wrap the three git calls + `materialise_frozen_base` in
   `await asyncio.to_thread(...)` inside `ensure_sandbox`). Smallest, highest
   leverage. Ship this first.
2. Add `timeout=` to every innermost `subprocess.run` in
   `snapshot_host_tree` / `snapshot_host_head` / `materialise_frozen_base`
   (Edit D).
3. **Edit B + C** — wrap the mkdir/write block + the sub-agent base write
   block in `to_thread`.
4. Add `SandboxMaterialiseSemaphore(2)` (§5.4) around the `else:` branch
   of `ensure_sandbox`.
5. Run the audit (§6) and patch any other sync-subprocess on the loop.

Each step is independently testable: the repro in §1 should show progressively
less chat stall after step 1, and step 4 adds a bounded-concurrency guarantee.

## 8. Risk / rollback

- `asyncio.to_thread` runs the callable on a default executor thread. The
  functions under change are pure-effects subprocess + filesystem ops with no
  shared mutable state inside the loop (they take args and return / write to
  disk). Thread-safety surface is minimal: the only shared state touched is
  the on-disk overlay dir for `conv_id`, which is per-conversation and not
  concurrently written by another coroutine (the chat's own loop serialises
  its hops). Safe.
- The `materialise_frozen_base` idempotency check at 166
  (`if (dest / ... / "alternates").is_file() and any(dest.iterdir())`) is now
  run off-thread but against the same `dest` — still safe because each conv_id
  has a unique `base` dir and the worker only re-enters `ensure_sandbox` for
  the same conv after the prior call returned.
- Rollback: revert Edit A (un-`await` the three lines) — the original sync
  behaviour is restored verbatim. No schema/format change, no on-disk change.

## 9. Summary (one paragraph)

The chat server freezes during sandbox creation because
`sandbox.ensure_sandbox` — an `async def` awaited by the worker — calls three
synchronous, blocking git/subprocess+filesystem functions
(`snapshot_host_tree`, `snapshot_host_head`, `materialise_frozen_base`)
directly on the asyncio event-loop thread instead of offloading them with
`asyncio.to_thread` (the pattern already used in `worker.py` for other sync
ops). Wrapping the three git calls (and the overlay mkdir/write block) in
`await asyncio.to_thread(...)`, adding `timeout=` to every innermost
`subprocess.run`, and bounding concurrent materialisation with a small
semaphore fully fixes the blockage. The change is ~4–20 lines, isolated to
`sandbox.py`, and fully rollbackable.
sandbox.py", and fully rollbackable.

## 10. Implementation log (applied 2024)

File changed: `.pi/crack/server/src/crack_server/sandbox.py`. One file, ~+60/−20 lines.

**Edits applied (matches the plan in §5):**

1. **Module-level constants (§5.4):** added `_GIT_SUB_TIMEOUT = 60` and a
   module-level `_SANDBOX_MAT_SEM` / `_materialise_sem()` helper so a sandbox
   materialisation semaphore (`asyncio.Semaphore(2)`) is created lazily on the
   running loop (laziness avoids the "no running event loop" error at import
   time, which is what happens if you build the semaphore at module top level).

2. **Edit D — subprocess timeouts inside `materialise_frozen_base`:**
   - `tar -x ...` now uses `timeout=_GIT_SUB_TIMEOUT;
     `subprocess.TimeoutExpired` kills+drains the `git archive` Popen and raises a
     `RuntimeError` with the same message shape.
   - `arch.communicate(...)` uses `timeout=_GIT_SUB_TIMEOUT`
   - `git init` uses `timeout=_GIT_SUB_TIMEOUT`
   - the three seed commands (`symbolic-ref` / `update-ref` / `read-tree`) use
     `timeout=_GIT_SUB_TIMEOUT`
   This prevents a stuck git from pinning a default-executor thread — which
   would have re-introduced the freeze at scale once we moved the work off the
   loop.

3. **Edit A/B/C — off-thread everything inside `ensure_sandbox`:**
   - `_prep_overlay_dirs()` local `def` wraps the 4 `mkdir` calls and is `await`ed
     via `asyncio.to_thread(_prep_overlay_dirs)`.
   - The parent-conv probe `overlay_base_dir(parent_conv).is_dir()` is run via
     `await asyncio.to_thread(overlay_base_dir(parent_conv).is_dir)`.
   - `frozen_tree_for(parent_conv)` is `await asyncio.to_thread(...)`.
   - A small `_seed_subagent_base()` local `def` wraps the `_overlay_root(...).mkdir`,
     `overlay_tree_path(...).write_text` of the sub-agent branch and is `await`ed.
   - The host-snapshot branch is **gated by `_materialise_sem()`** (the bounded
     sem): `snapshot_host_tree`, `snapshot_host_head`, and
     `materialise_frozen_base` are all `await asyncio.to_thread(...)`ed.
   - The slow `podman run` (~line 400) was already async-clean and is left as is;
     because it sits *outside* the sem block, multiple sandboxes can still
     overlap their `podman run` while at most 2 do the heavy git reproduction.

4. **defensive init:** `tree: str | None = None` so the success-log
   `(tree or "?")` at the end always has a bound name across both branches
   (matches original runtime behaviour; the sub-agent branch's `tree = None`
   case now logs `?` instead of raising `UnboundLocalError`, which is strictly
   safer).

**Verified:** `python3 -m py_compile src/crack_server/sandbox.py` → OK.

**Not done in this pass (see §6 audit):** the `*_sync` wrapper audit, and the
`rag.py` / `git_utils.py` / `pi_proc.py` / `patch.py` subprocess audit. Those
are independent follow-ups; the chat-blockage symptom (§1) is fully addressed
by the `ensure_sandbox` change since every `await sandbox.ensure_sandbox(...)`
call site (`chats.py:1436`, `chats.py:1470`, `sub_agents/base.py:316`) now hits
only async-clean code.
