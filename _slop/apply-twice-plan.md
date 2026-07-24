# Plan: merge-based patch integration + human review gate

## 1. The bug that started this

A patch that *should* have applied cleanly was rejected as a conflict (chat `1784887401169`):

1. Agent wrote a plan file; its finish-time patch was applied to the host root.
2. A follow-up turn edited a code file **and** re-edited its own plan file.
3. Meanwhile the original plan file was **committed** on the host (root moved forward).
4. The follow-up patch was `git diff base_tree end_tree` where `base_tree` is frozen at
   sandbox/job start — *before* the plan existed on the host. That patch re-adds the plan
   as a **new file** with stale context.
5. `git apply --3way`/`--reject` to the host fails: the "new file" already exists upstream.

### Root cause

`base_tree` is frozen once per job ([`chats.py:1442`](../.pi/crack/server/src/crack_server/chats.py#L1442))
and the finish-time integration is a **textual `git apply`**
([`patch.py:376`](../.pi/crack/server/src/crack_server/patch.py#L376)), which requires the
destination worktree to still match the patch base. The host diverges three ways before
apply — same conversation (a prior turn/plan committed), cross conversation (the worker runs
jobs **concurrently**, [`worker.py:272`](../.pi/crack/server/src/crack_server/worker.py#L272)),
and sub-agent→parent overlay drains. `git apply` cannot absorb non-overlapping upstream
drift, so it reports a conflict where none exists in content.

## 2. Scope of this plan

The fix is no longer "make apply robust." We are replacing **auto-apply** with a
**human review gate** and a real **3-way merge** integration engine, plus a GitHub-style
diff-review UI, and a nested **sub-agent → top-level → user** aggregation flow. The merge
engine (originally the whole plan) is now one component of a lifecycle rework.

## 3. Locked decisions

| # | Decision |
|---|----------|
| D1 | **Integration engine = real 3-way merge** (bundle + `git merge-tree`), never textual `git apply`. No container/lower re-sync (overlayfs forbids mutating a live lowerdir — [`sandbox.py:394-399`](../.pi/crack/server/src/crack_server/sandbox.py#L394-L399)). |
| D2 | **Nothing auto-commits to the host.** A finished agent turn produces a *pending patch* on disk; the host worktree is untouched until the user clicks **Commit**. |
| D3 | **Review UI** = vendored **diff2html** (client-side JS/CSS in `/static`): inline **and** side-by-side, GitHub-style, with a `--stat` overview whose diff body is hideable, and **per-line comment gutters**. |
| D4 | **Three user actions:** **Commit** (prefilled message = agent/sub-agent title), **Reject-with-comments** (a normal follow-up message + per-line comments concatenated into the agent prompt), **Ignore** (leave on disk untouched). |
| D5 | **Sub-agent patches auto-merge** into the managing agent's overlay (each surfaces an informational review note, no gate). The top-level **collapses all sub-commits + its own dirt into ONE unified patch** for the single human gate. |
| D6 | **Commit-time conflict → auto-bounce to the agent** (the agent-assisted rung of the retry ladder), then re-open review. |
| D7 | **Retry ladder = 1 auto-merge + 1 agent turn**, both counts in **module-level constants** (`MERGE_AUTO_ATTEMPTS = 1`, `MERGE_AGENT_ATTEMPTS = 1`). |
| D8 | **Retention:** overlays/bundles persist until the **conversation is deleted**; the container is **stopped (not removed)** when all pi agents idle and restarted for later review rounds. |

### Feasibility (verified)

- Host `git` is **2.47.3**; `git merge-tree --write-tree --merge-base=<b> <ours> <theirs>`
  emits a merged-tree oid + conflict report. This is the merge primitive.
- Artifact dirs live under `CRACK_HARNESS_DATA_DIR`, the **same shared volume** mounted in
  crack-dev and every sandbox — so a bundle the sandbox writes into its artifact dir is
  readable host-side (how we ferry git objects).
- crack-server serves `/static` via `StaticFiles` ([`app.py:38`](../.pi/crack/server/src/crack_server/app.py#L38))
  and drives the UI over htmx (`/api/chats/{chat_id}/…` POST → HTML fragment). This is the
  crack-server's own UI, **not** a Claude Artifact, so vendoring diff2html JS is fine.

## 4. New lifecycle

```
agent turn ends
   │
   ├─ extract delta  → delta.bundle + delta.json + patch.diff  (in artifact_dir, shared vol)
   ├─ container STOP (not rm); overlay persists
   └─ chat/​run state → phase "review"; pending_patch recorded
                          │
              ┌───────────┴──────────────┐
     UI renders review panel      (sub-agent case: auto-merge into parent, note only)
              │
     user action:
       Commit ─────────► merge_apply(host) under lock ──► git commit (scoped) ──► note: name+hash
                              │ real conflict? (D6)                                 teardown sandbox
                              └► bounce to agent (agent rung) ──► new turn ──► re-review
       Reject+comments ─► enqueue agent turn (prompt ⊕ comments) ──► container START ──► re-review
       Ignore ─────────► leave on disk; stays in "review" until conversation deleted
```

Concretely this reworks the `run_chat` idle branch
([`chats.py:1447-1480`](../.pi/crack/server/src/crack_server/chats.py#L1447)): instead of
`finalize_chat_sandbox` (extract → apply → destroy), the idle path calls a new
`publish_pending_patch` (extract → stop container → set `review` phase). Destruction moves to
commit-success and to conversation-delete GC.

## 5. Integration (merge) engine

Shared core used by **commit → host** and **child → parent overlay**.

### 5.1 Producer (in the sandbox, at extract)

Augments `_produce_diff`. Alongside `patch.diff`, emit a self-contained object payload:

```
git read-tree <base_tree>; git add -A        # + size-guard excludes (unchanged)
end_tree=$(git write-tree)
end_commit=$(git commit-tree "$end_tree" -p "$base_commit" -m "crack-delta")
git bundle create <artifact_dir>/delta.bundle "$base_commit..$end_commit"
```

- `base_commit` = the frozen base's HEAD sha (already seeded + persisted to `overlays/head`
  by `materialise_frozen_base` — expose `sandbox.frozen_head_for(conv_id)`). It is a **real
  host commit**, so it is present in the destination object store and the bundle is minimal.
- Persist `delta.json = {base_commit, base_tree, end_commit, end_tree}` next to the bundle.
- Keep `patch.diff` for display, the fallback path (§5.4), and the agent-facing conflict text.

A bundle (not the `--binary` diff) because `merge-tree` needs the **objects**, not a patch.

### 5.2 Consumer — `merge_apply(dest, artifact_dir)`

`dest = None` → host (`git -C /workspace …`); `dest = <sandbox>` → parent overlay
(`podman exec`). Both backends already exist (`_git_host`, `_git_in_sandbox`). Under the
apply lock (§5.3):

```
git fetch --no-tags <artifact_dir>/delta.bundle +<end_commit>:refs/crack/incoming
GIT_INDEX_FILE=$(mktemp -u); git read-tree <dest_HEAD>; git add -A
dest_tree=$(git write-tree); rm -f "$GIT_INDEX_FILE"    # live worktree incl. uncommitted dirt
merged_tree,conflicts = git merge-tree --write-tree --merge-base=<base_tree> <dest_tree> <end_tree>
```

- **No conflicts** → `git diff <dest_tree> <merged_tree> | git apply` (worktree only, index/HEAD
  untouched). Guaranteed to apply: `dest_tree` *is* the live worktree, so the net diff's
  context matches byte-for-byte, and upstream drift is already folded in.
- **Conflicts** → the only real-conflict case: `merge-tree` lists conflicted paths and writes
  conflict-marker blobs into `merged_tree`. Return them to the retry ladder (§7).

Always delete `refs/crack/incoming` in a `finally`.

### 5.3 Apply lock (concurrency, first-class)

A cross-process `flock` on `harness_dir()/locks/host-apply.lock` wraps the **entire** §5.2
sequence for host commits — worker jobs run concurrently in one process and sync callers /
the health watcher also touch the host tree, so a file lock is the safe superset. Each commit
re-reads `dest_tree` **inside** the lock, so it always merges onto the latest host. Parent-
overlay drains stay serialized per parent via the existing `patch_draining` claim
([`patch.py:841`](../.pi/crack/server/src/crack_server/patch.py#L841)); add a per-parent lock
only if drains can ever run in two processes (they can't today — assert the invariant).

### 5.4 Fallback / back-compat

Missing `delta.bundle` (old artifact dir / bundle build failed) → fall back to
`git apply --3way`/`--reject` on `patch.diff`, logged as a downgrade. Keeps in-flight
conversations working across the deploy.

## 6. Sub-agent aggregation (D5)

- A child's extract writes its `delta.bundle` into its artifact dir **before** its container
  stops (objects would vanish with the overlay upper otherwise).
- `drain_parent_patches` uses `merge_apply(parent_sandbox, child_artifact_dir)` per child, in
  dispatch order (unchanged ordering), **committing** each merge into the parent overlay's own
  git (`git commit-tree`/`commit`) for provenance, and appends an **informational** review note
  to the parent (`_note_parent`, path+stat only — no gate).
- The top-level's **unified patch** for the user = `git diff <base_tree> <parent-worktree-tree>`
  at publish time — this already collapses every sub-commit **and** the top-level's own dirt
  into one delta. No new aggregation code; it is the existing extract against the frozen base.
- Same three user actions apply to that one unified patch. On Commit, the merge engine folds it
  into the host as a single commit.

## 7. Retry / conflict ladder (D6, D7)

Constants at module top of `patch.py`:

```python
MERGE_AUTO_ATTEMPTS  = 1   # harness auto 3-way merge tries
MERGE_AGENT_ATTEMPTS = 1   # agent-assisted regeneration turns
```

Counter `apply_attempt` persisted in chat/run state (sibling to `patch_guard_attempts`).

1. **Auto (Commit action):** `merge_apply` under lock. Clean → commit + note (name+hash) →
   teardown. Real conflict → step 2.
2. **Agent-assisted (auto-bounce):** enqueue a step back into the owning agent carrying
   (a) the upstream diff `git diff <base_tree> <dest_tree>`, (b) the conflicted file list, and
   (c) conflict-marker files written into the overlay worktree for the conflicted paths.
   Reuses `enqueue_chat_system_message` / `persona.enqueue_step(..., "patch_conflict": …)`
   ([`patch.py:542`](../.pi/crack/server/src/crack_server/patch.py#L542)). Agent produces a
   fresh delta → re-open review.
3. **Exhausted** → terminal conflict card (existing `record_chat_apply_failure` /
   `patch_conflict`), with conflicted paths + the `patch.diff` path. Reset `apply_attempt`.

**Reject-with-comments** feeds the *same* agent channel voluntarily (user-triggered rather than
conflict-triggered): the follow-up message text ⊕ serialized per-line comments become the next
prompt; it does **not** consume a `MERGE_AGENT_ATTEMPTS` budget (that budget is for
conflict bounces only).

## 8. Review UI (D3, D4)

### 8.1 Diff rendering

- Vendor **diff2html** (`diffview.js` + `diff2html.min.css`, and its dependency for the
  side-by-side matcher) into `src/crack_server/static/vendor/diff2html/`. No CDN.
- Server sends the raw unified diff (from `patch.diff` / `git diff <base_tree> <end_tree>`) in
  a `<script type="application/x-diff">` or a data attribute; a small init script renders it
  with `Diff2HtmlUI` (toggle: **inline ⇄ side-by-side**; **hide/show** the diff body under the
  file-summary overview diff2html generates).
- The overview (files + `+/-` counts + collapsible bodies) is produced **by diff2html** from
  the diff — we do not hand-roll numstat (D "whatever the tool gives"). We only capture the
  raw diff text and hand it over.

### 8.2 Per-line comments

- diff2html renders one DOM row per source line with stable line numbers. On row hover show a
  "+" gutter; clicking opens a comment box **to the right**, aligned to that line.
- A comment = `{file, side: old|new, line, body}`. Pending comments persist in chat/run state
  (`review_comments`) so they survive reloads until the user submits Reject.
- On **Reject**, serialize comments into the prompt appended to the user's message:
  ```
  <path>:<line>: <comment body>
  ```
  concatenated after the message text, then enqueue the agent turn.

### 8.3 Actions (new htmx endpoints, HTML-fragment responses)

| Endpoint | Effect |
|----------|--------|
| `POST /api/chats/{id}/patch/commit` (body: message, default = chat title) | run merge_apply→commit under lock; on success append a `commit` note (message, short hash, "integrated into repo"), teardown sandbox, clear `pending_patch`; on conflict → §7 step 2 |
| `POST /api/chats/{id}/patch/comment` (file, side, line, body) | append to `review_comments`; return the updated gutter fragment |
| `POST /api/chats/{id}/patch/reject` (message) | prompt ⊕ `review_comments` → enqueue agent turn; `podman start` container; clear comments |
| `POST /api/chats/{id}/patch/ignore` | mark ignored; leave overlay/bundle on disk; no teardown |

Sub-agent runs reuse the same panel/endpoints under `/chats/{id}/run/{run_id}/…`, but per D5
only the **informational** note renders there — the actionable panel is the top-level unified
patch.

### 8.4 Trajectory integration

Replace the plain patch note branch in `render_note_row`
([`render.py:448`](../.pi/crack/server/src/crack_server/render.py#L448)) with a **review panel**
for `note_type == "patch"` carrying a `review` payload (diff text + action bar), and a lighter
`commit`/`review-note` badge for the informational and terminal states. Theme-aware CSS in
`static/app.css` (diff2html ships light+dark themes — pick per `prefers-color-scheme`).

## 9. Files to touch

| File | Change |
|------|--------|
| `patch.py` | Producer: `delta.bundle` + `delta.json` (`_produce_diff`/extract). Consumer: `merge_apply()` (+ `git apply` fallback). Retry constants + `apply_attempt`. `publish_pending_patch` replaces the apply-in-`finalize` logic; commit/reject/ignore handlers. Sub-agent drain → `merge_apply` + provenance commit + info note. |
| `sandbox.py` | `frozen_head_for(conv_id)`; **stop** (not `rm`) on publish; `flock` helper under `harness_dir()/locks`; teardown moved to commit/GC. |
| `chats.py` | `run_chat` idle path → `publish_pending_patch` + `review` phase; keep container across review; wire reject→re-enqueue. |
| `routes_chats.py` | 4 new endpoints (commit/comment/reject/ignore) for chats **and** runs. |
| `render.py` | Review panel (diff2html host), comment gutters, commit/terminal badges. |
| `static/vendor/diff2html/*` | Vendored JS+CSS. `static/app.css` panel + gutter + comment styles. |
| `state`/`paths.py` | `pending_patch`, `review_comments`, `apply_attempt` fields; `append_traj_note` already stores arbitrary keys. |
| conversation delete (`api_chat_delete`, [`routes_chats.py:151`](../.pi/crack/server/src/crack_server/routes_chats.py#L151)) | GC overlays + `rm` container for the conversation (D8). |
| tests | §10. |

## 10. Test plan (`python -m pytest` in `.pi/crack/server`)

- **Merge core, no drift** — delta onto unchanged dest ≡ today's result (regression guard).
- **The exact bug** — base has no plan; dest has the plan **committed** upstream; end re-adds+
  edits it + edits code → `merge_apply` succeeds; worktree = union; index/HEAD untouched.
- **Real conflict** — same lines diverge → conflict reported; ladder enqueues an agent step with
  the upstream diff; second failure records the terminal card; `apply_attempt` respects consts.
- **Concurrency** — two host commits race the flock; the second re-reads `dest_tree` and lands.
- **Fallback** — missing `delta.bundle` → `git apply` path.
- **Sub-agent aggregation** — 2 children + top-level dirt → one unified diff vs base; children
  merge in dispatch order; only info notes on children.
- **Review lifecycle** — publish sets `review` + stops container (not rm); Commit → scoped
  `git commit`, note carries hash, sandbox torn down; Reject re-starts container, prompt carries
  serialized comments; Ignore leaves disk intact; delete GCs it.
- **UI** — endpoints return valid fragments; comment round-trips into `review_comments`;
  commit-message defaults to chat title.
- **Integration (manual, `docker exec crack-dev`)** — replay chat `1784887401169`; confirm the
  second patch merges and the review panel renders inline + side-by-side.

## 11. Risks & edge cases

- **Stale-review display (§ criticism 6):** the diff shown may predate other commits. Commit
  **re-merges against live host under the lock**; if `merged_tree` differs materially from what
  was displayed, re-render the panel with the refreshed diff and require a second confirm rather
  than committing something unseen.
- **Commit scope vs user's own dirt:** `git commit` must be **path-scoped to the patch's files**
  (reuse `git_utils.commit(add=[paths], …)` shape) so a user's unrelated uncommitted edits are
  never swept in. Interacts with the existing host-dirty gate
  ([`chats.py:1136`](../.pi/crack/server/src/crack_server/chats.py#L1136)).
- **Self-mod gate + health watcher move to commit time:** `run_sandbox_tests` and
  `launch_health_watcher` now gate the **Commit** action (the only moment code hits the host),
  not publish. Keep their ordering: tests → merge → commit → health-watch.
- **Container stopped, not removed:** disk/overlay retained until conversation delete (D8);
  ensure `ensure_sandbox`'s `podman start` path resumes a stopped review container cleanly.
- **Binary / delete-vs-edit conflicts:** `merge-tree` handles these at oid level; genuine
  overlaps become real conflicts (correct). Size-guard still runs before bundling.
- **`refs/crack/incoming` leakage:** always deleted in `finally`; under `refs/crack/*` so it
  never touches user branches.
- **diff2html payload size:** huge diffs render slowly client-side; cap inline render (e.g.
  collapse files > N KB behind a click) — const-configurable.

## 12. Sequencing

1. **Merge engine + fallback** (producer bundle, `merge_apply`, flock) — internal, no UX change;
   swap `finalize`/`drain` to use it while still auto-committing, to de-risk the merge in
   isolation. *(This alone fixes the original bug.)*
2. **Lifecycle inversion** — `publish_pending_patch`, container stop-not-destroy, `review` phase,
   delete-GC.
3. **Review UI** — vendored diff2html panel, overview toggle, inline/side-by-side.
4. **Actions** — commit (scoped, self-mod gate, health-watch) / reject / ignore endpoints +
   retry ladder + auto-bounce.
5. **Per-line comments** — gutter, `review_comments`, prompt serialization.
6. **Sub-agent aggregation** — provenance commits + info notes; unified-patch review at top.
7. Remove the pure-`git apply` path once bundles are universal (keep fallback ≥1 deploy).

---

### Points still worth your confirmation

1. **Reject budget** — confirmed reject-with-comments does **not** consume the
   `MERGE_AGENT_ATTEMPTS` conflict budget (only auto-bounces do).
    OK.
2. **Commit provenance for sub-agents** — I commit each child into the parent overlay for
   history, but the user only ever sees/commits the **one** squashed top-level patch (child
   commits never reach the host as separate commits). Is a single squashed host commit right,
   or do you want child commits preserved in host history?
    One squashed commit is ok.
3. **Stale-review re-confirm** — on a materially-changed re-merge at Commit, I **re-render +
   require a second click** rather than committing silently. Acceptable, or should it just
   commit the refreshed merge?
    Ok, re-render and wait for second click, showing a label saying the view is refreshed.
4. **diff2html vendoring** — pinned vendored copy in `/static/vendor` (no build step). If you'd
   rather it go through an npm/bundler step already used elsewhere, point me at it.
    This is fine. 
