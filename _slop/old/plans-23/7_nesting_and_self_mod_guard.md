# Plan 7 — Chain-overlay nesting + self-modification apply guard

> Read `0_overview.md`; requires Plans 1–6. Two finishing pieces: (a) a sub-agent's sandbox
> must see the parent's **uncommitted** work (chain overlays), and (b) applying a top-level
> patch that touches server code must not brick crack-dev.

## Part A — Chain-overlay nesting

### Problem

A sub-agent sandbox today (`ensure_sandbox`) overlays the **pristine host repo** as its
lowerdir. But a sub-agent should start from the **parent's current tree**, including the
parent's uncommitted edits — otherwise the parent's context and the child's starting point
diverge, and the auto-applied child patch (Plan 4) is computed against the wrong base.

### Design

When the conversation is a **sub-agent run** (`parent_kind == "run"` or a chat-parent that
already has an overlay), build the child's lowerdir from the **parent's merged view** instead
of the host repo. With overlayfs you can stack lowers: the child's lowerdirs are
`parent_upper : host_repo` (upper of parent, then the pristine host as the deepest lower),
with a fresh child upper on top. In podman terms, a single `:O` mount can't express a
multi-lower stack directly, so use one of:

- **Option A (recommended): explicit overlay mount.** Instead of `-v host:/workspace:O`, do a
  manual overlay: `--mount type=overlay,destination=/workspace,lowerdir=<parent_upper>:<host_repo>,upperdir=<child_upper>,workdir=<child_work>`.
  `<parent_upper>` is the parent sandbox's persisted upper on the volume
  (`/crack-harness-data/overlays/<parent_id>/upper`) — **stable because Plan 6 freezes the
  parent while this child runs**. `<host_repo>` is `CRACK_HOST_REPO_ROOT`. Child upper/work
  are fresh dirs under `/crack-harness-data/overlays/<child_id>/`.
- **Option B (simpler, heavier): materialize.** At spawn, snapshot the parent tree
  (`git -C /workspace add -A && git write-tree` in the parent, then check that tree out into a
  fresh dir on the volume) and use that as the child's single lower. Costs a checkout per
  spawn but avoids multi-lower subtleties.

Go with **Option A** unless overlay-on-overlay ordering causes trouble; document whichever you
ship. Key correctness point either way: the parent MUST be frozen (Plan 6) for the child's
lower to be stable — verify Plan 6 is active before enabling nesting.

### `ensure_sandbox` change

Add a `parent_id: str | None` parameter. When set, resolve `<parent_upper>` and build the
chain overlay; when `None` (top-level chat), keep the plain `:O` over the host repo. The
caller (`sub_agents/runner.py:spawn` → the run_start step) passes the parent conversation id.

### Depth note

`MAX_DEPTH = 1` today, so the only chain is chat(0) → subagent(1): the child's lower is the
**chat sandbox's** upper. If `MAX_DEPTH` is later raised, the same rule recurses (each level's
lower = its parent's merged view, each parent frozen while its child runs). Don't hardcode a
two-level assumption; derive `<parent_upper>` from the parent's id generically.

### Verification

1. **Child sees parent's uncommitted edit.** Chat task: "Create `/workspace/PARENT_ONLY.txt`
   containing HELLO. Then spawn a `<persona>` sub-agent whose task is: read
   `/workspace/PARENT_ONLY.txt` and report its contents." The sub-agent's report must contain
   `HELLO` — proving the child's lower includes the parent's uncommitted file.
   (Because of Plan 6, the parent's file write happens *before* the spawn here, or the parent
   freezes appropriately; sequence the task so the write precedes the spawn.)
2. **Child edit comes back correctly.** The child edits `PARENT_ONLY.txt`; after wait_join,
   Plan 4 auto-applies the child delta to the parent overlay; confirm the parent sees the
   child's change.
3. **Isolation still holds:** the host `/workspace/PARENT_ONLY.txt` never exists during the
   run (all in overlays).

## Part B — Self-modification apply guard

### Problem

crack-dev runs the live server/worker and auto-reloads on code change. When a **top-level
chat** edits server code, Plan 4 applies that patch to the real `/workspace` — which reloads
crack-dev. A broken patch crashes the very process doing the apply, with no path back.

The sandbox already makes *iterating* safe (the agent's edits stay in its overlay; crack-dev
doesn't reload mid-run). This part guards the **final apply-to-host** step.

### Design

Before applying a top-level patch that touches `.pi/crack/server/**` or
`.pi/extensions/crack/**`:

1. **Test in the sandbox first.** In the finished sandbox (still alive), run the server's
   tests against the overlay tree:
   `podman exec <sbx> bash -exc 'cd /workspace/.pi/crack/server && uv run pytest -q'`.
   If they fail, **do not apply to host** — inbox the failure + patch path to the chat and let
   the agent fix (same conflict-style handoff as Plan 4). 
2. **Snapshot for rollback.** Record the pre-apply host state: `git -C /workspace stash create`
   (or a `write-tree`) so you can restore if the reload wedges.
3. **Apply + health-check.** Apply the patch, let crack-dev reload, then poll
   `GET http://127.0.0.1:9847/` for HTTP 200 within a deadline (e.g. 60s). 
4. **Rollback on failure.** If the health-check fails, restore the snapshot
   (`git -C /workspace checkout` / reset to the recorded tree) so crack-dev reloads back to a
   good state, and report the failure + patch path to the chat.

### Who runs the guard

The apply/health-check must survive crack-dev restarting. Two viable owners:
- Run the health-check + rollback from the **anchor container** `crack-harness-data` (it's
  always up, mounts the volume, and can `podman exec crack-dev` / curl crack-dev). A small
  script there watches for a "pending apply" marker on the volume and performs step 3–4.
- Or accept a brief window: crack-dev's worker writes the rollback marker + snapshot ref to
  the volume *before* applying, and on reboot the entrypoint checks: if a marker exists and
  the server didn't come healthy, auto-rollback. Simpler; do this if the anchor-driven path is
  too fiddly.

Document which you ship. For non-server patches (the common case), skip the guard entirely and
apply directly (Plan 4 behavior).

### Verification

1. **Good server patch:** chat task that makes a trivial, correct server change (e.g. add a
   comment or a harmless log line to a server file), completes, tests pass in sandbox, applies
   to host, crack-dev reloads, health-check 200. Confirm the change is live.
2. **Bad server patch:** chat task that introduces a syntax error in a server file. Confirm:
   sandbox tests fail → **no host apply** → chat gets the failure + patch path; crack-dev
   stays healthy throughout (never crashed).
3. **Forced-reload rollback:** if you implement step 3–4, simulate a patch that passes tests
   but breaks boot (e.g. bad import only hit at runtime); confirm the health-check fails and
   the rollback restores a healthy crack-dev.
4. **Conceptual check (write in report):** confirm the end-to-end story — an agent can now
   edit the harness, iterate in its sandbox without crashing the live server, and only a
   tested+healthy patch reaches the host. This is the "work on itself" goal.

## Gotchas

- Overlay-on-overlay (Option A) requires the parent upper to be a real dir on the same
  storage; it is (on the volume). Test that a child can both read parent files and write its
  own without EXDEV/link errors.
- The self-mod guard's rollback must not itself depend on the crashed server — that's why the
  anchor container or the entrypoint-on-reboot owns it.
- Tests-in-sandbox use the sandbox's overlaid `/workspace/target` (`:O`) — first run may be a
  cold cargo/pytest; that's fine, it's the agent's own overlay.

## Report

`_slop/report-23/7_nesting_and_self_mod_guard.md`: the nesting option (A/B) + why, the
self-mod guard owner (anchor vs entrypoint), all verification results (chat ids), and an
explicit paragraph confirming the "harness works on itself" goal is met (or exactly what's
still missing).
