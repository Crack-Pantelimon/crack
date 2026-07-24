# Plan 7 report — Chain-overlay nesting + self-modification apply guard

> Implemented by the review/finish agent on top of the committed parts 1–6
> (`6386b3c`). Also contains the parallel-sub-agent patch guard the user asked for,
> plus three bug fixes surfaced while wiring Part 7 together.

## TL;DR

- **Part A (nesting):** podman rootless rejects the plan's Option-A multi-lower
  `--mount type=overlay`, and an explicit `:O` `upperdir` can't sit on the host's
  overlay root. Shipped a **git-replay** materialization instead: the child sandbox
  starts as a plain `:O` overlay over the host repo (like a chat) and the parent's
  uncommitted delta is `git apply`-ed into it (`patch.seed_child_from_parent`).
- **Parallel-patch guard:** a finishing child no longer applies its patch straight
  into the parent overlay. `extract_run_patch` defers (flags `patch_pending`) and
  `drain_parent_patches` applies all pending child patches **in dispatch order**,
  **serialized** by an atomic `patch_draining` claim, **only when
  `active_child_count == 0`**. This closes exactly the corruption window the user
  described (a child applying while siblings still run).
- **Part B (self-mod guard):** a top-level patch touching `.pi/crack/server/**` or
  `.pi/extensions/crack/**` is **tested in the sandbox first**; a failing suite means
  **no host apply** + a message to the chat. A successful apply launches a **detached
  health-check watcher** that reverse-applies the patch if crack-dev never comes healthy.
- **Bug fixes (blocking / correctness):** (1) `CRACK_PI_HOST=0.0.0.0` leaked into
  sandboxes and broke *all* spawn/wait/ask from sandboxed chats — the likely reason
  the Plan 6 test "cut mid-way"; (2) `transcript.py` `KeyError('thinking')` on a
  provider `message_end` with no `turn_start`; (3) the chat idle-finalize destroyed
  the chat sandbox while sub-agents were still running.

## Part A — chain-overlay nesting (git-replay, not multi-lower)

### Why not the plan's Option A

Verified live on this host:

- `--mount type=overlay,...,lowerdir=A:B,...` → `Error: invalid filesystem type "overlay"`
  (rootless podman, `fuse-overlayfs` not even installed; graph driver is kernel `overlay`).
- The host `/` is itself `overlay`; a `:O` overlay whose `upperdir` is created *directly*
  on a host path fails the merge mount. (The existing `:O` sandboxes work only because
  the upper dirs are created *through* crack-dev's `/crack-harness-data` bind mount.)

So a child **cannot** mount the parent's persisted upper as a lower. Chosen mechanism:

### git-replay seed (`patch.seed_child_from_parent`)

On a sub-agent's **first hop only** (`base_tree` absent), before capturing its own
baseline, the child sandbox is seeded from the parent:

1. In the **parent** sandbox, compute the delta vs the parent's baseline using a
   throwaway `GIT_INDEX_FILE` (so sibling seeds never contend on the parent's real
   `.git/index`): `read-tree <base>; add -A; diff --binary <base> <write-tree>`.
2. Write it to `<run>/parent_seed.diff` and **plain** `git apply` it in the child
   (plain, not `--3way/--reject`, since the child tree matches the base exactly — no
   stray `.rej` files that would pollute the child's own baseline).

Best-effort: a failed seed logs and leaves the child on the pristine host tree.

**Consequence for the parallel concern:** because each child overlays the *pristine
host* (seeded once at spawn), siblings no longer share the parent's live upper as a
mutable lower — so the "overlay lower must be stable" UB is structurally avoided. The
remaining hazard is two children `git apply`-ing into the *same parent* sandbox at
once; the drain (below) serializes that.

### Verification

- **Scripted E2E (definitive, isolated ids):** parent sandbox creates
  `PARENT_ONLY.txt=HELLO` **and deletes** `_docker/run.sh`; child seeded →
  child sees `HELLO`, sees the deletion as MISSING, still has `Dockerfile`, and **no
  `.rej` files**. Host stayed clean throughout.
- **Live wiring (nemotron-super):** after the `CRACK_PI_HOST` fix, a sandboxed chat
  successfully spawned coder sub-agents that ran through `base._run_hop` (which calls
  `seed_child_from_parent`). *Gap:* I could not get the weak nemotron-super to reliably
  do **write-then-spawn** in one turn, so no single live chat produced a *non-empty*
  seed diff — the parent had no uncommitted delta at spawn time in the runs that did
  spawn. The mechanism is proven by the scripted E2E; the live runs prove the path
  executes. Honest gap, not a code defect.

## Parallel-sub-agent patch guard (the user's ask)

`runner.finish` now: `extract_run_patch(mark_pending=True)` (extract + persist
`patch.diff` + tear down the child sandbox, **no apply**) → mark terminal →
`drain_parent_patches`. The drain:

- returns immediately if `active_child_count > 0` (a still-running sibling will drain
  last);
- atomically claims `patch_draining` on the parent state (`JsonState.update` = flock'd
  RMW) so racing sibling `finish()` threads can't double-apply;
- applies every `patch_pending` child in **ascending run-id order** (= dispatch order)
  into the parent overlay, clearing each flag (even on conflict, which is handed to the
  managing agent exactly like Plan 4);
- loops so a sibling that finishes mid-drain is still picked up.

Cascade stop uses `extract_run_patch(mark_pending=False)` (stopped children never push
into the parent).

### Verification

- **Unit tests** (`test_patch.py`): dispatch-order apply, defer-while-siblings-running,
  conflict-notifies-and-clears.
- **Scripted E2E through the real `finish()`/drain:** child A finishes while B runs →
  A's patch **deferred** (`SIB_A` absent on parent, `patch_pending=True`). B finishes →
  **both** `SIB_A` and `SIB_B` land on the parent overlay, both flags cleared.

## Part B — self-modification apply guard

`finalize_chat_sandbox` gates the host apply when `patch_touches_self_mod(patch)`:

1. **Test in sandbox** (`run_sandbox_tests`): `uv run --no-sync pytest -q` against the
   overlay (the sandbox inherits the host's synced `.venv` via `:O`). Fail → **no host
   apply** + `format_test_failure` message to the chat (`source=patch_tests`). Verified
   live: 142 tests run inside a real sandbox.
2. **Apply + health watcher:** on success apply to host, then launch the detached
   `_docker/_apply_healthcheck.sh` (owner: **entrypoint-independent detached process**
   inside crack-dev — survives the uvicorn reload it watches). It polls
   `http://127.0.0.1:9847/`; if unhealthy past the deadline it **reverse-applies**
   (`git apply -R`) so the reloader recovers, logs to
   `/crack-harness-data/harness/apply_rollback.log`, and drops a breadcrumb in the chat dir.

Non-self-mod patches skip the whole gate (Plan 4 behavior).

### Verification

- **Scripted E2E:** BAD (break `tests/test_patch.py` in the sandbox) → sandbox tests
  fail → host file **UNCHANGED** + chat got `patch_tests`. GOOD (add a harmless server
  module) → tests pass → applied to host (then cleaned). Final host tree pristine.
- **Watcher rollback (isolated):** apply a bad hunk, point the watcher at a dead port →
  after the deadline it reverse-applies and the file returns to pristine; log shows
  `UNHEALTHY … reverse-apply OK`.
- *Gap:* I did **not** break the real crack-dev boot to test the live reload→rollback
  path (too risky mid-session); the watcher mechanics are proven in isolation.

## "Harness works on itself" — end-to-end story

An agent editing `crack_server/**` now iterates entirely in its sandbox overlay (the
live server never reloads mid-run). Only when the chat idles is a self-mod patch
**tested in that overlay**; a failing suite never reaches the host, and a passing patch
that nonetheless breaks boot is auto-reverted by the watcher. Combined with Part A
(sub-agents inherit the parent's in-flight edits) and the parallel guard (their patches
merge back deterministically), the harness can safely modify its own code. **Met**, with
the two honest gaps above (live non-empty seed diff; live reload-rollback).

## Bug fixes shipped alongside

| File | Bug | Fix |
|------|-----|-----|
| `pi_proc.py` | `_spawn_sandbox_pi` passed `CRACK_PI_HOST=os.environ[...]` = `0.0.0.0` (crack-dev's uvicorn *bind* addr) into the sandbox, so the extension built `http://0.0.0.0:9847` → every spawn/wait/ask from a sandboxed chat failed `ECONNREFUSED`. Almost certainly why the Plan 6 live test "cut mid-way". | Pin `CRACK_PI_HOST="crack-dev"` (the crack-net hostname the container already sets). |
| `transcript.py` | `current_turn["thinking"] += …` raised `KeyError('thinking')` when a `message_end` arrived with no preceding `turn_start` (nemotron does this), failing the whole hop. | Accumulate text/thinking with `.get(...)` defaults. |
| `chats.py` | Chat idle-finalize destroyed the chat sandbox while sub-agents still ran → their finish-time patches had no parent overlay. | `_has_active_runs` guard holds the sandbox open; `drain_children` re-finalizes later. |
| `index.ts` | Plan 6 `hasRunningChildren` probe hard-failed every destructive tool on any transient server blip (e.g. a reload). | Retry 3×, then **fail open** (safe: git-replay children don't mount the parent's live tree). |

## Final review pass (nemotron-3-ultra)

Ran `nvidia/nemotron-3-ultra-550b-a55b` over the drain logic. Three findings, all
addressed:

1. **`patch_pending` cleared before `git_apply`** → a *raised* apply (e.g. podman
   timeout) would strand the patch. **Fixed:** clear only *after* a definitive apply;
   on exception, leave pending + `continue` (per-child `try/except`), so a later drain
   retries. Regression test `test_drain_apply_exception_leaves_pending`.
2. **Lexicographic sort ≠ dispatch order.** True only if epoch widths ever differ;
   made it explicitly numeric via `_dispatch_key` (parse the ms-epoch head).
3. **Drainer crash strands the loser's patches.** Mitigated by the per-child
   `try/except` (one bad apply no longer aborts the whole drain) plus a **no-progress
   break** so the re-loop can't spin on a persistently-failing apply.

Residual (documented, low-impact): if two children finish their hops in the *same
instant*, the apply order can deviate from strict dispatch order — but both still apply
and any conflict still surfaces, and no apply ever happens while a sibling's hop is
still running (the safety invariant holds unconditionally).

## Commands / tests

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. uv run pytest -q tests/ --ignore=tests/test_vision_media.py'
# 143 passed (135 baseline + 8 new: drain order/defer/conflict/apply-exception, self-mod detect, test-failure msg)
```

Sample chats live under `/crack-harness-data/unscripted_chats/` (deleted in the
post-run cleanup per the task). Scripted probes were run through the real code paths
(`crack_server.patch` / `runner.finish`) rather than the flaky provider.
