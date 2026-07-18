# Crack-server: worker engine, git, implementation & review stages

## Context

`crack-server` (`.pi/crack/server`) is a FastAPI + htmx app that drives a task through
an ordered pipeline of **stages** (Explore → Plan → Plan Review → Implementation),
auto-discovered from `src/crack_server/stages/sNN_*.py`. Each stage persists its state
to a per-task JSON file, renders an htmx-polled panel, and today does its slow `pi`
work in **background threads spawned inside the uvicorn web process**. Stages auto-chain
(explore → plan → plan_review), but the chain only ever starts because a browser POSTs
to a route, and the threads die if uvicorn auto-reloads.

This change set (from `_slop/pi-crack-server.md`) does seven things:

1. Fix the plan-review → implementation transition bug (stale panel needs a manual refresh).
2. **Move all `pi` execution out of the web process into a separate, reentrant, flock'd
   worker process fed by an on-disk command queue** (chosen: full migration). Progress
   continues even with the UI closed.
3. Show **elapsed seconds** next to the in/out byte counts already shown per action.
4. Audit rate-limiting / waits for speed.
5. Auto-**git-commit** at defined checkpoints (best-effort, errors ignored).
6. Turn the static Implementation handoff into a **real agent stage** (kimi-k2.6 →
   glm fallback, with turn caps, repeated-error switching, and 5-turn todo reminders).
7. Add an **Implementation Review** stage and a **Finished** stage (walkthrough +
   retrospective + a chat box that continues the review session).

Decisions locked with the user: full worker migration; switch models on 2 consecutive
turns with the same failing tool error (or >10 turns); the Finished chat resumes the
review `pi` session (tools enabled).

> Model-id note (per project memory): `nvidia/z-ai/glm-5.2` is already used by Plan
> Review, so it is known-good. **`nvidia/moonshotai/kimi-k2.6` must be verified with
> `pi --list-models` inside the container** before wiring it as the default; fall back
> to the closest listed id if absent. `pi` is not on the host, so verify in-container.

---

## Suggested build order (phases)

- **Phase A** — Item 1 (bug fix) + Item 3 (timing) + Item 4 (rate audit): small, isolated,
  independently shippable, no worker dependency.
- **Phase B** — Item 2 (worker + queue): the foundation everything else rides on.
- **Phase C** — Item 5 (git), then Item 6 (implementation agent), then Item 7 (review +
  finished), each of which runs on the worker built in Phase B.

---

## Item 1 — Fix plan-review → implementation stale panel

**Cause.** On approve, `api_stage_action` (`app.py:738`) returns Plan Review's fragment +
an OOB glyph, but the **Implementation panel is never re-rendered** — it was drawn once at
page load (`polling=False`, showing "Approve the plan first…") and `app.js` `onAfterSwap`
(`static/app.js:96`) only `.click()`s the impl tab, revealing stale content. Hence the
"header + still says accept" until a manual refresh.

**Fix.**
- Add an `oob: bool = False` param to `Stage.wrap_status` (`stages/base.py:139`). When true,
  add `hx-swap-oob="true"` to the content `<div>` (it already carries `id="<slug>-content"`),
  so htmx replaces the live panel out-of-band.
- In `api_stage_action` (`app.py:738`), when `slug == "plan_review" and action == "approve"`,
  append `stages.get("implementation").render_status(task_id, oob=True)` alongside the
  existing glyph OOB swap. Because Implementation is now an active polling stage (Item 6),
  the freshly-swapped panel immediately begins polling — no refresh needed.
- Keep the `app.js` tab-activation branch; it now reveals fresh, polling content.

---

## Item 3 — Elapsed seconds per action

`render_actions_table` (`stages/base.py:538`) renders one row per action; the Size column
shows `in …/out …` but no timing because turns store no timing.

- In `pi_runner.run_agent_hop` (`pi_runner.py:360`), stamp `time.monotonic()` on the
  `turn_start` event and per `toolCall` id; on `turn_end` / `toolResult` compute deltas and
  attach `elapsed` (float seconds) to the persisted turn dict and to each `tool_block`.
  (Do the timing in the loop, not in the pure `apply_event_to_turn`.)
- In `base._render_text_action_row` / `_render_tool_action_row`, append `· {elapsed:.1f}s`
  to the Size cell when present (thread the turn's `elapsed` into the text-row call).
- Single-shot calls (`run_pi_text` already logs `elapsed`): return/stash it and surface it
  in the relevant stage meta line (e.g. Plan's "done" meta at `s02_plan.py:360`, "· final Ns").
  Best-effort — "show it when we have it".

---

## Item 4 — Rate-limit / wait audit

Finding from exploration: the **only** `sleep` in the server is `RateLimiter.wait`
(`pi_runner.py:56`). It already computes `sleep_for = min_interval - (now - last_call)` and
skips sleeping when the previous call took longer than the interval — i.e. it already "only
sleeps if the last request came in under the budget", which is exactly the requested behavior.

Deliverable is therefore a light audit + tweaks, not a rewrite:
- Confirm/keep the deficit-only sleep; add a log line making the "no wait needed" path visible.
- The title model applies **two** limiters (nvidia 40rpm + title 30rpm) sequentially; ensure
  they don't double-count — the nvidia limiter's `last_call` is shared, so this is fine, but
  document it. No static per-request sleeps exist to remove.

---

## Item 2 — Worker engine + on-disk command queue (full migration)

**New: `src/crack_server/queue.py`** — a filesystem job queue under
`.pi/crack/harness/queue/` (matches the existing JSON-state-file convention; no new infra dep):
- Job spec JSON: `{ "id", "task_id", "slug", "step", "form": {...}, "enqueued_at" }`.
- `enqueue(task_id, slug, step, form=None)` → writes `pending/<ms>_<uuid8>.json` atomically.
- `claim_next()` → atomically `os.rename` the oldest pending file into `processing/`; returns
  the job or None.
- `complete(job)` / `fail(job)` → remove the processing file.
- `reclaim_orphans()` on startup → move `processing/*` older than a threshold back to pending
  (reentrancy after a watchfiles restart / crash).

**Stage base changes (`stages/base.py`)**
- Add `Stage.enqueue_step(task_id, step, form=None)` (thin wrapper over `queue.enqueue`).
- Add `Stage.run_step(self, task_id, step, form)` — dispatch entrypoint the **worker** calls;
  default raises. Each stage maps `step` → its existing internal `_run_*` method.
- `Stage.start()` default → `self.enqueue_step(task_id, "start")`.

**Mechanical migration in every stage** — replace each
`threading.Thread(target=self._run_X, args=(task_id, arg)).start()` with
`self.enqueue_step(task_id, <step>)`, and implement `run_step` to call `_run_X` **synchronously**:
- `s01_explore.py`: `start()` writes initial state (fast, keep) then enqueues `"run"`;
  `run_step("run")` → `_run_job`. Its tail `plan_stage.start(...)` already enqueues — good.
- `s02_plan.py`: `start`→enqueue `"draft"`; `submit_answers` keeps the fast answer-write then
  enqueues `"draft"`; `run_step` maps `draft`→`_run_draft_step`, `final`→`_run_final`
  (the in-method `self._run_final(...)` call becomes `enqueue_step("final")`).
- `s03_plan_review.py`: `start`→`"critique"`; `handle_action` keeps fast state writes, enqueues
  the step; `run_step` maps `critique/followup/grill/revise/reject` to `_run_review_step(step)`
  (internal `self._run_review_step(task_id,"revise")` → `enqueue_step("revise")`).
- Title regen (`app.py:383` `_start_title_regen_job`): enqueue a `("__title__","title")` job
  instead of spawning a thread; worker runs `_run_title_regen_worker`. (Register a tiny internal
  handler in the worker for the non-stage title job, or model title as a pseudo-stage step.)

**The web process no longer spawns any pi-executing thread** — routes call `stage.start()` /
`stage.handle_action()` which only do fast disk writes + `enqueue`, then return the (soon-to-poll)
fragment. All `pi` subprocesses now run in the worker, so the process-global `RateLimiter` in
`pi_runner` naturally governs the whole system from one process.

**New: `src/crack_server/worker.py`** (console script `crack-worker`)
- `_loop()`: `reclaim_orphans()`, then poll `claim_next()` every ~0.5s; dispatch each job to
  `stages.get(slug).run_step(...)` (or the title handler) inside a bounded
  `ThreadPoolExecutor(max_workers=4)` so multiple tasks interleave while sharing the rate limiter;
  on exception set that stage's error state + `queue.fail`, else `queue.complete`.
- `main()`: `from watchfiles import run_process; run_process(<pkg dir>, target=_loop)` — mirrors
  uvicorn `reload=True` so worker code edits auto-restart it.

**Deps / packaging**
- `uv add watchfiles` in `.pi/crack/server` (uvicorn[standard] already vendors it, but declare it).
- Add `crack-worker = "crack_server.worker:main"` to `[project.scripts]` in `pyproject.toml`.

**Docker start (`_docker/_cont_start.sh`)** — start the flock'd, auto-refreshing worker in the
background *before* the web server:
```bash
cd /workspace/.pi/crack/server
uv sync
export CRACK_PI_PROJECT_ROOT=/workspace
# single-instance, auto-refreshing worker (flock: if already held, exit 0)
( flock -n /workspace/.pi/crack/harness/worker.lock uv run crack-worker || true ) &
uv run crack-server
```
(Create `harness/` if missing; `flock -n … || true` gives the requested "else exit 0" behavior.)

---

## Item 5 — Git commits at checkpoints

**New: `src/crack_server/git_utils.py`** — `commit(add, message)`:
`git -C <project_root> add <paths…>` then `git -C <root> commit -m "slopmaster3000: <message>"`,
each wrapped so **any** failure is logged (`logger.error`) and swallowed. `add` accepts a path or list.

Checkpoints:
- **Prompt change** (web process) — in `api_create_prompt` / `api_update_prompt` /
  `api_delete_prompt` (`app.py:570/595/619`): `commit(<prompt file>, f"change prompt file {name}")`.
- **First plan ends** — end of `s02_plan._run_final` success: `commit(task_dir, "plan complete <id>")`
  (add the whole task folder, per spec).
- **Each round of work preview** — after each Implementation/Review hop is persisted (Items 6/7):
  `commit(task_dir, "work round <n> <id>")`.
- **Implementation accepted / review done** — on stage completion: `commit(task_dir, …)`.

Cross-process git races are handled by the swallow-and-continue contract.

---

## Item 6 — Implementation as a real agent stage (rewrite `s04_implementation.py`)

Replace the static handoff renderer with an agentic stage modeled on Plan Review's hop loop.

- **Parts / models** (configurable via `harness/implementation.json`):
  `Part("primary", …, "handoff.md", "nvidia/moonshotai/kimi-k2.6")`,
  `Part("fallback", …, "handoff.md", "nvidia/z-ai/glm-5.2")`.
- **State**: new `implementation.json` + helpers in `paths.py`
  (`read/write_implementation_state`, `implementation_sessions_dir`, `walkthrough_path` →
  `plan/walkthrough.md`). Phases: `idle → running → done | error`.
- **Auto-start**: in `s03_plan_review._approve`, after setting `done`, `commit(task_dir, …)` and
  `stages.get("implementation").start(task_id)` (enqueues). (Also the Item-1 OOB refresh.)
- **`run_step("run")` → `_run_implementation`**:
  - Build the message from the existing `_assemble_handoff` + instructions to keep
    `plan/walkthrough.md` (what was done / problems / fixes) and update `todo.md`.
  - Session id `impl-<task>`, tools `bash,read,edit,write`, small `IMPL_TURNS_PER_HOP` (~3) so
    switch-checks + commits happen each "round of work preview".
  - Loop hops with `pi_runner.run_agent_hop`, `model=current_model` (starts primary). After each
    hop: persist turns (reuse the plan-review `persist` pattern with Item-3 timing), then
    `commit(task_dir, …)`.
  - **Switch to fallback** when `total_turns > 10` **or** two consecutive turns share the same
    failing-tool signature: `_turn_error_signature(turn)` = `(tool_name, normalized_error_output)`
    over tool_blocks whose output looks failed (nonzero exit / "error" / "Traceback"); equal on
    two in a row ⇒ set `current_model = fallback` for subsequent hops (same session id, new
    `--model`; verify pi resumes a session under a different model in-container).
  - **Every 5 turns** inject: "Update your todo file at `{todo_path}` and reply with its full path."
  - Completion sentinel `IMPLEMENTATION_COMPLETE`; also overall turn/time caps. On done: mark
    `done`, `commit`, and `stages.get("impl_review").start(task_id)`.
- **Rendering**: reuse `render_turns_trajectory` (now with seconds) + show `walkthrough.md`;
  polling while running. Status feeds the tab glyph like other stages.

---

## Item 7 — Implementation Review stage + Finished stage

**New `stages/s05_impl_review.py`** (slug `impl_review`)
- `Part("reviewer", …, "review.md", "nvidia/z-ai/glm-5.2")` (configurable).
- New template dir `prompt_templates/implementation_review/review.md`: load full plan context
  (final_plan, todo, walkthrough, explore summary, original prompts), instruct to run `git diff`,
  validate/test **everything** (build, tests, demos) while respecting the plan, be critical,
  fix wrong code at will, keep updating `plan/walkthrough.md` + `todo.md`, and **loop on any
  compiler warnings / test failures** until clean. Tools `bash,read,edit,write`; sentinel
  `REVIEW_COMPLETE`; session id `review-<task>`.
- `state`: `impl_review.json` + paths helpers. Enabled when implementation is `done`.
  `run_step("run")` → hop loop like Item 6 (commit each hop). On done → mark done (Finished unlocks).

**New `stages/s06_finished.py`** (slug `finished`)
- `status`: `awaiting` until `impl_review` done, then `done`.
- `render`: show `walkthrough.md` (the retrospective) + final plan + the review trajectory, and a
  **chat box** at the bottom.
- Chat: a form POSTs action `chat` (`msg`) → handler enqueues `run_step("chat", form)` which
  **resumes the `review-<task>` pi session with tools enabled** (continues context, can inspect/
  edit), appends turns to `finished.json`, and renders them as a polled trajectory. Uses the
  generic `/api/tasks/{id}/stages/{slug}/actions/{action}` route — no new app.py route needed.

Both new stages are picked up automatically by the registry (`stages/__init__.py`) from their
`sNN_` filenames; no `app.py` route changes (the generic stage routes cover start/status/actions).

---

## Files touched (summary)

- **New**: `queue.py`, `worker.py`, `git_utils.py`, `stages/s05_impl_review.py`,
  `stages/s06_finished.py`, `prompt_templates/implementation_review/review.md`.
- **Rewritten**: `stages/s04_implementation.py`.
- **Edited**: `stages/base.py` (enqueue/run_step, `wrap_status` oob, timing in rows),
  `pi_runner.py` (per-turn/tool timing; rate-log tweak), `stages/s01_explore.py`,
  `stages/s02_plan.py`, `stages/s03_plan_review.py` (thread→enqueue; git; auto-start next),
  `app.py` (prompt-commit; approve OOB refresh; title job → queue), `paths.py` (new state/paths +
  `harness/queue`), `models.py` (no change expected), `static/app.js` (verify tab reveal),
  `pyproject.toml` (`watchfiles` dep + `crack-worker` script), `_docker/_cont_start.sh` (flock worker).

## Verification

Do this **inside the container** (that is where `pi`, models, and the real pipeline live):
1. `pi --list-models | grep -iE 'kimi|glm|moonshot'` — confirm/repair the two impl model ids.
2. Rebuild/start via `_docker/run.sh`; confirm in logs that `crack-worker` starts, holds the
   flock (start a 2nd copy → it exits 0), and that `uv run crack-server` starts.
3. Create a task, add a prompt → verify a `slopmaster3000: change prompt file …` commit appears
   (`git log --oneline`).
4. Drive Explore → Plan → Plan Review **with the browser closed after kicking it off**; reopen and
   confirm the worker advanced stages (proves UI-independent progression). Confirm a plan-complete
   commit landed.
5. Approve the plan → **no manual refresh**: Implementation panel swaps in and starts polling
   (Item 1). Watch kimi run, switch to glm after >10 turns or a repeated tool error, get 5-turn
   todo reminders, write `plan/walkthrough.md`, and commit each round.
6. Implementation done → Review auto-runs `git diff`, tests/builds, loops on warnings, updates
   walkthrough. Then Finished shows the retrospective; type in the chat box and confirm it resumes
   the review session (tools active) and appends to the trajectory.
7. Confirm every agent action row shows `in …/out … · N.Ns` (Item 3), and that touching worker
   source auto-restarts the worker (watchfiles), reclaiming any in-flight job (Item 2 reentrancy).
