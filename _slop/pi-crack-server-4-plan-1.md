# Plan 4.1 — Runner & stage lifecycle (backend only)

One of three independent plans (4.1 runner/lifecycle, 4.2 web transport/UI, 4.3
refactor + residual bugs). This part touches **only** `pi_runner.py`, the six
`stages/s0N_*.py` backends, `stages/base.py` action handling, `queue.py`, `worker.py`,
`paths.py` (new helpers), and `prompt_templates/`. It changes **no** polling/HTML
structure beyond adding two buttons/forms — plan 4.2 owns the transport and rendering.
It is implementable and testable on its own with a fake `pi` shim (see Test plan).

Bug refs (B*) are from `_slop/pi-crack-server-5-next-bugs.md`. This plan absorbs the
still-unimplemented items §1–§4 and §6 of `_slop/pi-crack-server-2-plan-1.md` (checked
against the current tree: none of them landed).

---

## 1. Record every compiled prompt into the trajectory (data contract for 4.2)

**Why:** the exact prompt sent to pi is only in server logs today
(`pi_runner.py:551`, `158`); the UI shows only what comes out.

**Contract (shared with plan 4.2 — do not deviate):** stage/chat `turns` lists may now
contain, in addition to assistant-turn dicts, *prompt entries*:

```json
{"kind": "user_prompt", "label": "hop 2" | "turn_zero" | "gate" | "summary" | "final" | …,
 "template": "explore.md",            // template basename, "" when ad-hoc
 "compiled": "<full message string>", // exactly what was passed to pi
 "hop": 2, "at": 1789000000.0}
```

Assistant turn dicts stay exactly as they are (no `kind` key) so old state files render
unchanged. Renderers must skip dicts whose `kind` they don't know.

**Changes:**

- `pi_runner.run_agent_hop` gains a `record_prompt` callable param (default `None`).
  At the top of the call (before `_attempt`), invoke
  `record_prompt({"kind": "user_prompt", "compiled": message, "hop": hop, "at": time.time()})`.
  Callers pass a closure that fills `label`/`template` and appends via the same persist
  path as turns (so prompt entries land in `state["turns"]` in order, before the turns
  they produced). Record once per hop *call*, not per retry attempt.
- `run_pi_text` similarly gains optional `record_prompt`; used by explore turn-zero /
  gate / summary (`s01_explore.py:273/332/377`), plan final (`s02_plan.py:293`), todo
  regen (`s03_plan_review.py:136`). Title calls (`app.py:427`, `chats.py:310`) do NOT
  record (no trajectory to show them in).
- Each stage call site: pass the closure. For the persist closures that write
  `existing + new_turns`, append prompt entries to `new_turns` the same way.
- Chats: same for `chats.run_chat` (`chats.py:376`) and `s06_finished._run_chat`
  (`s06_finished.py:113`) — record the *compiled* `chat.md` message; the raw user
  message is already stored as `exchange["user"]` (that is the "original prompt" the
  expandable row reveals alongside the compiled one; store `original` on the prompt
  entry when the compiled text was built from a single user message:
  `{"kind": "user_prompt", "original": user_msg, ...}`).

**Acceptance:** run any stage with the shim; the stage's state JSON contains
`kind: "user_prompt"` entries interleaved in `turns`, one per pi invocation, whose
`compiled` matches the shim's received argv prompt byte-for-byte.

---

## 2. Provider-keyed rate limiter, lock-free waits (plan-2 §1; B22-adjacent)

- `pi_runner.py:89-136`: replace `_nvidia_limiter` + `wait_for_rate_limit`:
  - `def limiter_for(model: str) -> RateLimiter | None`: parse `provider = model.split("/", 1)[0]`;
    return a shared 40 rpm limiter for `provider == "nvidia"` (created lazily in a
    dict guarded by a module lock), `None` otherwise.
  - Keep the per-model `TITLE_MODEL` 30 rpm limiter, applied only when the model is
    nvidia-hosted (it is; keep the generic `_model_limiters` dict).
  - Rewrite `RateLimiter.wait` to reserve-and-release: under the lock compute
    `slot = max(now, self._next_free); self._next_free = slot + self._min_interval`,
    then sleep `slot - now` **outside** the lock. Threads no longer serialize on the
    lock during sleeps.
- Both call sites (`run_pi_text:171`, `_attempt:559`) go through the new
  `limiter_for(model)`; non-nvidia models make back-to-back calls.

**Acceptance:** shim test — two concurrent hops on `moonshotai/x` show < 0.2s spacing;
40 sequential `nvidia/x` calls take ≥ ~58s aggregate.

---

## 3. Transient-error retries that resume the session (plan-2 §2)

- Add `def is_transient(text: str) -> bool` scanning combined stdout/stderr tail for:
  `ResourceExhausted`, `429`, `rate limit`, `overloaded`, `temporarily`,
  `503`, `502`, `504`, `connection reset`, `connection refused`, `ETIMEDOUT`,
  case-insensitive.
- `run_pi_text` (`pi_runner.py:139-208`): on nonzero exit where
  `is_transient(detail)`, retry on the schedule but *extend* it: transient failures get
  their own backoff `[20, 45, 75]`s (crosses the upstream per-minute window) in
  addition to the existing 4-attempt/61s schedule for hard process failures. Simplest:
  when transient, override `_retry_backoff_sleep` with the transient offsets.
- `run_agent_hop` retry loop (`pi_runner.py:737-777`):
  - Current behavior: `persisted > 0` → break → raise (`766-773`). Change: when the
    failure is transient (`is_transient(res["detail"])`), retry **even with persisted
    turns** — the pi session dir is preserved and the next attempt re-invokes with the
    same `--session-id`/`--session-dir` and the *continuation message*
    `"Continue where you left off."` instead of replaying `message` (add a local
    `attempt_message` variable; first attempt uses `message`). This is the resume-not-
    replay path. Non-transient failures with persisted turns keep today's raise.
  - At least 3 transient reattempts (`20/45/75s`) before raising `PiError`.
- Give the two remaining single-shot subprocess helpers the same treatment or an
  explicit exemption comment: sigmap pre-queries (`s01_explore.py:135-169`, local tool —
  exempt) and `pi --list-models` (`models.py:32` — wrap with 2 quick retries).

**Acceptance:** shim emits `ResourceExhausted` twice then streams normally → stage
completes with one logical trajectory; shim fails 4× transient → stage errors; killing
the shim mid-stream after 2 turns then succeeding → turns 1-2 kept, session resumed
(shim sees `--session-id` unchanged and a "Continue where you left off." prompt).

---

## 4. Remove all turn limits in all stages (plan-2 §4; obsoletes B9)

- `pi_runner.run_agent_hop`: delete `turns_per_hop`, `max_turns`, `total_turns` params
  and the cap terminations (`pi_runner.py:680-689`); a hop ends only on sentinel, stop,
  `agent_end`, time cap, or error. Keep `count_turn_groups` (display only).
- Delete constants + uses + spinner fractions:
  - `s01_explore.py:35-38` (`PI_EXPLORE_MAX_TURNS`, `EXPLORE_MAX_HOPS`,
    `EXPLORE_TURNS_PER_HOP`), loop checks at 298-304, spinner at 493-498 → show
    `Exploring… turn {n}`. Keep the gate as the only hop terminator (unlimited hops,
    but see the neutral-gate rewrite below).
  - `s02_plan.py:46-49` + nudge message 226-231 (keep a *single* "stop calling tools,
    emit questions or sentinel" nudge only when `agent_end` happened without either —
    that nudge is flow control, not a cap).
  - `s03_plan_review.py:37-40` + 245-251 same treatment.
  - `s04_implementation.py:35-39`: `IMPL_TURNS_PER_HOP`, `IMPL_MAX_TURNS`,
    `IMPL_SWITCH_TURN_THRESHOLD` turn-based demotion (fix B10 while here: keep the
    fallback switch **only** for two adjacent identical failing-tool signatures, and
    tighten `_ERROR_MARKERS` to hard markers: `traceback`, `command not found`,
    `no such file or directory`, `fatal:`, `exit code`, dropping bare
    `error/failed/cannot/not found`); todo reminder every 5 turns stays (it's a nudge).
  - `s05_impl_review.py:29-31`, spinner 286.
  - `s06_finished.py:25-28`, `chats.py:36-39`: drop `*_MAX_HOPS`/`*_MAX_TURNS`; loop
    `while reason == "hop_cap"` becomes `while reason == "agent_end_without_answer"` —
    actually simplest: with hop caps gone, `run_agent_hop` never returns `hop_cap`;
    chat loops become a single call (plus stop handling).
- Templates: remove cap wording — `explore/explore.md:1` ("AT MOST 5 tool turns"),
  `explore/gate.md:1,19` ("hops of at most 5 tool turns", "Bias strongly toward
  stopping" → neutral: "Reply DONE only when all reconnaissance dimensions (files,
  tooling, tests, skills, resources) are covered; otherwise list up to 3 bullets of
  what is missing."). Leave the "AT MOST 5 questions" wording alone — that caps
  *questions*, not turns.
- Keep stage timeouts as the only backstop; raise explore's to 1800s
  (`s01_explore.py:36`).

**Acceptance:** `rg "MAX_TURNS|TURNS_PER_HOP|MAX_HOPS|turn_cap|hop_cap" src/` returns
nothing (except changelog comments); a shim streaming 40 turns completes uncut.

---

## 5. Explore "retry from last error" resumes instead of restarting (plan-2 §3)

- `s01_explore.py:414-420` currently calls `self.start()` → `rmtree` of the session dir
  (`209`) and full state reset. Rewrite to mirror `s02_plan.retry_from_error`
  (`s02_plan.py:327-342`): require `status == "error"`, clear `error`/`error_detail`,
  set `status` back to `"running"`, and enqueue a new `"resume"` step that skips
  turn-zero/sigmap and re-enters the hop loop with the message
  `"Continue exploring where you left off."` — the pi session
  (`explore-<task_id>` under the preserved sessions dir) supplies the context.
  `_run_job` gets split so the hop-loop portion is callable from both `"run"` and
  `"resume"` (`run_step` dispatch at `s01_explore.py:232-236`).
- Sweep: confirm no other stage's retry path calls `start()` or `rmtree` (verified
  in the current tree: s02-s05 are correct; s06/chats have no retry path).
- Relabel the button "Continue from last error" (`base.py:662-676`).

**Acceptance:** force an explore error after ≥1 turn; click retry; `explore.json` keeps
prior turns, `explore/sessions/` keeps its jsonl, and the shim receives the resume
prompt with the same session id.

---

## 6. STOP + continue-message backend for every stage (plan-2 §6 backend; B14 fix)

- `paths.py`: `def stage_pid_file(task_id, slug) -> Path` →
  `tasks/<id>/<slug>.agent.pid`.
- Every stage `run_agent_hop` call site passes
  `pid_file=paths.stage_pid_file(task_id, self.slug)` and
  `stop_check=lambda: read_<stage>_state(task_id).get("stop_requested", False)`
  (s01:241, s02:205, s03:229, s04:237, s05:167, s06:113 — chats already do).
- `stages/base.py Stage.handle_action` (`138-146`) grows two generic actions:
  - `"stop"`: set `stop_requested=True` in the stage's state (each stage exposes
    `read_state/write_state` — add thin `state_read(task_id)/state_write(task_id, d)`
    methods per stage so base can do this generically), `kill_pid_file(...)`, set
    status/phase to a new `"stopped"` value. Add `"stopped"` to `STATUS_COLORS`
    (`base.py:33-40`) mapping to the error color family.
  - `"message"`: allowed when status is `stopped` **or** `error`; clears
    `error`/`error_detail`/`stop_requested` (fixes the B14 stale-error class
    generically), restores the running phase, and enqueues a `"user_message"` step
    whose runner calls the stage's hop loop with the user's text as the message,
    resuming the existing session.
- Stage loops handle the `"stopped"` reason from `run_agent_hop`: persist
  status/phase `stopped` and return without enqueueing more (today s04:274-275 would
  treat it as continue — audit all six).
- Minimal UI hooks (plan 4.2 restyles them, but the feature must be testable now):
  in each stage's `render_status`, while running render a STOP button posting the
  generic `stop` action; when `stopped` or `error`, render a one-textarea form posting
  `message` (copy the `s06_finished.py:209-217` form). Keep it plain.
- Also fix B2 here (same guard family): `chats.post_message` and s06 `"chat"` action
  refuse (render current state) when phase is already `chatting`.

**Acceptance:** with the shim sleeping mid-stream: STOP kills the process group within
~2s, state shows `stopped`, prior turns intact; posting a message resumes the same
session with the message as the next prompt; a second concurrent send is rejected.

---

## 7. Double-run guard + queue/worker hardening (B1, B4, B5, B6)

- **B1:** add `queue.enqueue_exclusive(task_id, slug, step, form)` used by
  `Stage.start`/`retry_from_error`/`message`: it scans `pending/` + `processing/` for a
  job with the same `(task_id, slug)` and, if found, returns None (logged). Scanning
  two small dirs is fine at this scale. Additionally make each `start()` write a
  `started_token = uuid` into state before enqueueing and have the worker step
  verify the token matches when it begins (stale duplicate jobs exit immediately).
- **B4:** on worker startup (`_loop`, before `reclaim_orphans`), kill orphaned agents:
  for every `tasks/*/**.agent.pid` and `unscripted_chats/*/agent.pid`, call
  `kill_pid_file`. Only then reclaim.
- **B5:** make requeued steps re-entrant: explore `"run"` skips turn-zero/sigmap when
  `state["questions"]` is already non-empty (jump into the hop loop with the resume
  message); plan `"draft"` skips straight to a resume hop when turns exist. Same
  pattern for s04/s05 (they already resume by construction — verify).
- **B6:** `worker._dispatch` except-path: after `queue.fail(job)`, best-effort flip the
  stage's state to error (`stage.record_dispatch_error(task_id, str(exc))` — new base
  method writing `status/phase = error`, `error = f"worker dispatch failed: …"`) so
  the UI never spins forever. For `TITLE_JOB_SLUG`/`CHAT_JOB_SLUG` write the
  corresponding error states (B16's stuck-"running" title regen included).

**Acceptance:** double-POST start → exactly one job file ever exists; `kill -9` the
worker mid-run, restart → old pi killed, job requeued, no duplicated turn-zero
artifacts; a forced `NotImplementedError` step lands the stage in `error` with the
dispatch message.

---

## 8. Runner robustness oddments (B8, B12)

- **B8:** sentinel matches only on its own line:
  `any(l.strip() == sentinel for l in current_turn["text"].splitlines())`
  (`pi_runner.py:645-648`). Update template wording stays "on its own line".
- **B12:** wrap the stream loop with a hard watchdog: spawn
  `threading.Timer(timeout_seconds + 60, proc.kill)` (cancel in `finally`) so a pi
  that hangs without output cannot wedge the hop forever.

---

## Implementation order within this plan

1. §2 limiter (isolated), §8 oddments.
2. §3 transient retries + resume.
3. §4 cap removal (mechanical, wide).
4. §5 explore resume, then §6 stop/message (shares the resume plumbing).
5. §7 queue/worker hardening.
6. §1 prompt recording last (touches every call site once the dust settles).

## Test plan (self-contained)

Build `tests/fake_pi.sh` on PATH ahead of real pi: reads a control file
(`FAKE_PI_SCRIPT` env) telling it per-invocation to (a) emit canned JSONL turn events,
(b) exit nonzero with `ResourceExhausted` on stderr, (c) sleep N seconds, (d) echo its
argv/session flags to a capture file. Run worker + server per
`.pi/crack/server/README.md`. Each § above lists its acceptance check; script them as
`pytest` cases where practical (rate limiter and retry classification are pure-python
unit-testable without the shim). Nothing in this plan requires a browser.
