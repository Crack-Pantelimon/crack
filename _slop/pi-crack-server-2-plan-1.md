# Plan: improve `.pi/crack` server — concurrency, retries, resume, no turn caps, explore prompt, stop/continue UI

Scope: `.pi/crack/server/src/crack_server/` (FastAPI app + worker + stage runner) and
`.pi/crack/server/prompt_templates/`. Two processes exist: the uvicorn web app (`main.py`)
and `crack-worker` (`worker.py`, `ThreadPoolExecutor(max_workers=4)`, `worker.py:25`).

---

## 1. Full concurrency for tasks/sessions, 40 rpm cap only for `nvidia/*`

**Current state:** jobs already run concurrently (4 worker threads, filesystem queue
`queue.py`), but `pi_runner.RateLimiter` (`pi_runner.py:89-116`) is a process-global
minimum-interval limiter applied to **every** pi call regardless of provider
(`wait_for_rate_limit`, `pi_runner.py:128-136`, called from `run_pi_text:170` and
`run_agent_hop:559`). Since harness configs now use non-nvidia providers
(`cursor/*`, `moonshotai/*` in `harness/plan.json`, `harness/implementation.json`),
this wrongly throttles everything to 40 rpm and is the real serialization point.
The limiter also holds a `threading.Lock` across the sleep, serializing all threads.

**Changes:**

- `pi_runner.py`:
  - Replace the single `_nvidia_limiter` with a per-provider limiter dict:
    `limiter_for(model)` parses `provider/model`, returns a 40 rpm limiter only for
    `provider == "nvidia"`, and `None` (no wait) for all other providers.
  - Keep `TITLE_CALLS_PER_MINUTE` handling only if the title model is `nvidia/*`;
    otherwise drop it (title calls go through the same provider-keyed path).
  - Fix the limiter implementation so it does not hold the lock while sleeping:
    compute the next allowed slot under the lock (reserve-and-return), sleep outside
    the lock. This makes the limiter a true token/rate gate instead of a global mutex.
  - Keep `NVIDIA_CALLS_PER_MINUTE = 40` (`pi_runner.py:33`) as the only budget constant.
- Verify no other global serialization exists: `worker.lock` (`paths.py:280`) is only a
  single-worker-instance flock — fine. No per-task lock exists; add a per-task+stage
  guard so the same stage of the same task can't double-run (today only a status check,
  e.g. `s01_explore.py:204-206`) — a simple per `(task_id, slug)` threading.Lock dict in
  `worker.py` or a "running" claim flag in the stage JSON checked+set under the queue's
  atomic rename. Different tasks must never block each other.

**Acceptance:** two tasks running stages at once with `moonshotai/*` models make pi calls
back-to-back with no 1.5 s spacing; `nvidia/*` models stay ≤ 40 calls/min aggregate.

---

## 2. Retry all LLM/pi calls on transient upstream errors (ResourceExhausted etc.)

**Current state:** all pi calls funnel through `run_pi_text` (`pi_runner.py:139-208`) and
`run_agent_hop` (`pi_runner.py:489-777`), both of which have retry loops
(`PI_RETRY_ATTEMPTS = 4`, `_retry_offsets`, ~61 s window, `pi_runner.py:41-79`). So no call
site lacks retry — the gap is that `ResourceExhausted: Worker local total request limit
reached` (surfaced in pi's stderr/exit) is not classified as transient-retryable, and the
fixed ≤61 s schedule lands inside the same upstream per-minute window. Also
`run_agent_hop` only retries when zero turns were persisted (`pi_runner.py:766-773`).

**Changes:**

- `pi_runner.py`:
  - Add transient-error detection: scan subprocess stderr/stdout (and stream error events
    in `run_agent_hop`) for `ResourceExhausted`, `429`, `rate limit`, `overloaded`,
    `temporarily`, `503`, `502`, `connection` errors → classify as transient.
  - Retry policy for transient errors: **at least 3 reattempts** with backoff that crosses
    the upstream window (e.g. 20 s, 45 s, 75 s), applied in *both* `run_pi_text` and
    `run_agent_hop`, including the case where turns were already persisted (resume the
    same `--session-id`/`--session-dir` instead of giving up — pi sessions are
    resumable, so a mid-stream transient failure should retry by continuing the session,
    not by erroring the stage).
  - Non-transient failures keep the current behavior (raise `PiError` with tail).
- Audit the remaining single-shot subprocess calls and give them the same helper:
  - sigmap pre-queries `s01_explore.py:135-169`
  - `pi --list-models` in `models.py:32`
- Verify every `run_pi_text` / `run_agent_hop` call site inherits this (they all route
  through the two functions: s01:241/273/332/377, s02:205/293, s03:136/229, s04:237,
  s05:167, s06:113, `app.py:427`, `chats.py:310/376`) — no per-call-site changes needed
  beyond confirming none catch-and-swallow `PiError` before the retry loop finishes.

**Acceptance:** killing the upstream or forcing a `ResourceExhausted` response produces 3
automatic reattempts and the stage continues; only after retries exhaust does the stage go
to `error`.

---

## 3. "Retry from last error" must continue the existing chat, not wipe it

**Current state:** `POST /api/tasks/{task_id}/stages/{slug}/actions/retry_from_error`
(`app.py:765-775` → `base.py:138-146`). Plan/plan-review/implementation/impl-review
already resume correctly (flip phase back, re-enqueue `error_step`, keep the pi session
dir — e.g. `s02_plan.py:327-342`). **Explore is the broken one:**
`s01_explore.py:414-420` calls `self.start()`, which `shutil.rmtree`s the session dir
(`s01_explore.py:209`) and resets all state — full history loss.

**Changes:**

- `s01_explore.py`:
  - Rewrite `retry_from_error` to mirror the other stages: record the failing step in
    state (already done at `s01_explore.py:404-412`), then on retry clear `error` /
    `error_step`, restore the running phase, and re-enqueue the failed step **without**
    touching `paths.explore_sessions_dir(...)` and without resetting the persisted turn
    arrays in `explore.json`.
  - Because the pi session files are kept, `run_agent_hop` resumes the same
    `--session-id explore-<task_id>` and the agent continues with full context.
- Sweep all six stages for any other `start()`-from-retry path or `rmtree` on recovery
  (`s01_explore.py:209` is the known one; confirm s02–s06 have none).
- UI: `render_retry_button` (`base.py:662-676`) label could become "Continue from last
  error" to reflect the new semantics (optional, cheap).

**Acceptance:** fail explore mid-stage, click retry, and the stage JSON keeps all prior
turns and the pi session dir still has its `*.jsonl`; the agent's next message references
earlier context.

---

## 4. Remove all turn limits in all stages

**Current state:** caps live in per-stage constants and are enforced centrally in
`run_agent_hop` (`pi_runner.py:680-689`, grouped counting via `count_turn_groups:308-322`).
Display bug: explore spinner shows `len(turns)/15` (`s01_explore.py:496`) while the cap
counts groups — hence "turns 37/15".

**Changes:**

- `pi_runner.py`: remove hop-cap and total-cap termination (680-689); keep
  `count_turn_groups` only if still useful for display, otherwise delete. A hop ends only
  on: agent sentinel/done, stop request, or error.
- Delete constants and their uses:
  - `s01_explore.py:35-38` — `PI_EXPLORE_MAX_TURNS`, `EXPLORE_MAX_HOPS`,
    `EXPLORE_TURNS_PER_HOP`; hop-loop check at ~300; spinner text at 496 (show just
    `turns {n}`); done-meta at 445.
  - `s02_plan.py:46-49` — `DRAFT_TURNS_PER_STEP`, `DRAFT_MAX_HOPS_PER_STEP`,
    `DRAFT_MAX_TURNS`; hop-cap nudge message `s02_plan.py:226-231`.
  - `s03_plan_review.py:37-40` — `CRITIC_TURNS_PER_STEP`, `CRITIC_MAX_HOPS`,
    `CRITIC_MAX_TURNS`.
  - `s04_implementation.py:35-39` — `IMPL_TURNS_PER_HOP`, `IMPL_MAX_TURNS`, the >10-turn
    fallback-model switch (38, 256-264), the every-5-turns todo reminder (39, 216);
    spinner at 378.
  - `s05_impl_review.py:29-31` — `REVIEW_TURNS_PER_HOP`, `REVIEW_MAX_TURNS`; spinner 286.
  - `s06_finished.py:25-28` — `CHAT_TURNS_PER_HOP`, `CHAT_MAX_HOPS`, `CHAT_MAX_TURNS`.
  - `chats.py:36-39` — `CHAT_TURNS_PER_HOP`, `CHAT_MAX_HOPS`, `CHAT_MAX_TURNS`.
- Remove the "AT MOST 5 tool turns" style wording from prompt templates
  (`prompt_templates/explore/explore.md` and any similar line in other templates).
- Also remove prompt-side hop/gate pressure that exists only to respect caps, e.g. the
  gatekeeper "bias strongly toward stopping" (`prompt_templates/explore/gate.md`) — keep
  the gate but make it neutral ("reply DONE only when the exploration goals are met").
- Keep stage *timeouts* (300 s / 900 s / 3600 s) as the only backstop against a
  genuinely hung subprocess — consider raising explore's 300 s since turns are now
  uncapped (decide: e.g. 1800 s).

**Acceptance:** grep for `MAX_TURNS|TURNS_PER_HOP|MAX_HOPS` returns nothing; a long
explore runs past 15 grouped turns without being cut; UI shows a plain turn count.

---

## 5. Prompt review — explore stage becomes read-only reconnaissance

Templates root: `.pi/crack/server/prompt_templates/` (loaded via `Stage.load_template`,
`base.py:90-95`; editable in the `/stages/<slug>` UI — note: edit the files, and re-sync
any copies already rendered into existing task dirs if templates are cached per task).

**Explore stage rewrite** (`prompt_templates/explore/`):

- `turn_zero.md` (the first prompt, `s01_explore.py:272-278`): currently asks for 2-10
  speculative Q/A pairs. Rewrite so the questions are shaped by the reconnaissance goal —
  the agent is told up front that the whole stage is **read-only location mapping**, so
  the questions should be varied across: where the relevant files/code live, what online
  resources/docs exist, what software/tooling is installed, how it's invoked, what test
  methodology and scripts exist, and which available skills might apply to the task.
- `explore.md` (hop-1 agent prompt, `s01_explore.py:290-295`): rewrite the mission to:
  - Goal is to find **where** things are and **what** they are: files, code, config,
    online resources — with `path:line` citations as today.
  - **Forbidden:** installing software, editing/writing any code or files, running
    destructive commands. State this explicitly and early.
  - **Allowed/encouraged:** probing what software exists (`which`, `--version`, package
    listings) and noting how it should be used; identifying existing test methodology
    and scripts; exploring available skills (MCP tools, skill listings) relevant to the
    task; using the full read-only arsenal (rg/fd/sigmap, read-only MCP tools).
  - Remove the turn-count instruction (see §4) and any "be quick / bias to stop" wording.
  - Keep the `EXPLORATION_COMPLETE` sentinel.
- `gate.md` (`s01_explore.py:322-347`): replace "bias strongly toward stopping" with a
  checklist-driven gate: DONE only if all reconnaissance dimensions (files, tooling,
  tests, skills, resources) are covered, else up to 3 bullets of what's missing.
- `explore_summary.md` (`s01_explore.py:367-383`): add required sections to the summary:
  locations map, available tooling + usage notes, testing methodology, relevant skills,
  open questions for the plan stage.
- Follow-up hop message (`s01_explore.py:348-353`): align wording with the read-only
  mission ("continue mapping, still missing: …").

**Other stages (lighter pass):** read each remaining template
(`plan/draft.md`, `plan/final_plan.md`, `plan_review/critique.md`, `grill_followup.md`,
`revise.md`, `reject.md`, `todo.md`, `implementation/handoff.md`, `impl_review/review.md`,
`finished/chat.md`, `title.md`) and fix only what conflicts with the above: remove
turn-limit references, and make sure nothing in later stages contradicts the explore
output format they consume.

**Acceptance:** an explore run on a sample task produces a summary listing file map,
installed-software probe results, test scripts, and relevant skills — with zero file
writes or installs during the run.

---

## 6. Server UI: STOP button + continue-message box on `/tasks/<task_id>/view/*`

**Current state:** stage view pages are server-rendered (`app.py:880-890`,
`_render_task_view_body:838-869`), status polled via htmx every 1.5 s
(`Stage.wrap_status`, `base.py:175-205`). Stop/continue exists **only for unscripted
chats**: `POST /api/chats/{chat_id}/stop` (`app.py:963-966` → `chats.stop_chat:251-268`)
using `stop_requested` flag + `pi_runner.kill_pid_file` (`pi_runner.py:459-486`), and
`run_agent_hop` already supports `pid_file`/`stop_check` params (`pi_runner.py:505-506`)
with a "stopped" outcome classification (716-722). The six stage call sites do **not**
pass those params today.

**Changes:**

- Runner plumbing (`pi_runner.py`, stage call sites s01:241, s02:205, s03:229, s04:237,
  s05:167, s06:113):
  - Give every stage a pid-file path under the task dir (e.g.
    `tasks/<id>/<stage>/agent.pid`, helper in `paths.py`) and pass
    `pid_file=` + `stop_check=` at every stage `run_agent_hop` call. `stop_check` reads a
    `stop_requested` flag from the stage's JSON state.
- Stage base (`stages/base.py`):
  - Add a generic `"stop"` action in `Stage.handle_action` (138-146) calling a default
    `stop_from_ui(task_id)`: set `stop_requested` in stage state, `kill_pid_file(...)`,
    let the worker thread unwind and record status `stopped` (new status value — add to
    the status map/colors, `base.py:33-40`).
  - Add a generic `"message"` action: validates the stage status is `stopped` (by user or
    by error), clears `error`/`stop_requested`, and enqueues a step that calls
    `run_agent_hop` with the stage's existing `--session-id`/`--session-dir` and the user
    message — i.e. the chat continues with full history (same resume mechanism as §3).
- Stage loops (s01–s06): handle the `"stopped"` outcome from `run_agent_hop` — today
  they'd mis-handle it as hop-cap/agent-end continuation (e.g. `s04:274-275`); on
  stopped, persist status `stopped` and return without enqueuing more hops.
- UI rendering:
  - While `status == running`: render a **STOP** button (POST to the stage `stop`
    action, htmx swap) next to the spinner in each stage's `render_status` — mirror
    `chats.py:183-193`. No message box while running.
  - When `status == stopped` (user stop **or** error — normalize error into the same
    view, keep the error text visible): render a message input box at the bottom, under
    the entire chat transcript and under the buttons, POSTing to the stage `message`
    action — mirror `chats.render_chat_form` (`chats.py:136-161`) and the in-stage
    precedent `s06_finished.py:60-72,209-217`.
  - When `status == done`: neither STOP button nor message box.
  - Error banner stays; "retry from last error" button (§3) coexists — the message box
    is the free-form version of the same continue path.
- Route wiring: reuse the existing generic action route
  `POST /api/tasks/{task_id}/stages/{slug}/actions/{action}` (`app.py:765-775`) — no new
  routes needed beyond confirming htmx targets/swap selectors for the form.

**Acceptance:** start explore, STOP appears; clicking it kills the pi process within a
couple seconds, status shows `stopped`, a message box appears under the chat; typing
"try again" resumes the same session with prior turns intact; when the stage completes,
both STOP and the box disappear.

---

## Implementation order

1. §2 retry classification (smallest, fixes the live pain).
2. §3 explore resume fix (closely related to §2 resume path).
3. §4 remove turn caps (touches all stages + templates).
4. §1 provider-keyed rate limiter + lock-free limiter.
5. §5 explore prompt rewrite (templates only, iterate on a sample task).
6. §6 stop/continue UI (largest; builds on §3's resume plumbing).

## Test plan

- Unit-ish: run worker + server locally (`.pi/crack/server/README.md` has the commands);
  use a fake `pi` shim on PATH that can emit `ResourceExhausted` on demand, sleep N
  seconds, and stream canned JSONL — this makes §1/§2/§6 testable without burning real
  API quota.
- Concurrency: launch two tasks with non-nvidia models, assert interleaved pi spawns and
  no global spacing; with a nvidia model, assert ≤ 40 calls/min.
- Retry: shim emits ResourceExhausted twice then succeeds → stage completes; three times
  → stage errors, then "continue" succeeds.
- Resume: fail explore, retry, diff `tasks/<id>/explore/sessions/` before/after (no
  deletion) and check turn count grows.
- UI: drive `/tasks/<id>/view/explore` with the browser MCP tools — STOP during run,
  message box appears only in stopped state, continue works, done hides both.
