# Plan / Plan-Review revamp — investigation results

Investigation of why task `1784450256833_blender_mcp` stalled in the Plan stage
(chat scrape: `_slop/pi-chat-stagnation.md`, UI page
`http://localhost:9847/tasks/1784450256833_blender_mcp/view/plan`).
No code was changed. Evidence base: on-disk state under
`.pi/crack/tasks/1784450256833_blender_mcp/`, the pi session jsonl, docker logs
of container `crack-dev` (from 10:45 UTC onward; earlier logs lost on container
recreate), git history of `.pi/crack`, and the live UI via curl + browser.

## TL;DR

Three independent failures stacked up:

1. **Harness bug (the decisive one): the Plan stage can never reach its `final`
   step.** `queue.enqueue_exclusive` treats the *currently executing* job as a
   duplicate, so when `_run_draft_step` finishes and enqueues the `final` step
   from inside the worker, the job is silently dropped. The stage sits in
   `final_running` ("Writing final plan…" spinner) forever, with no process
   behind it, until the user gives up and hits STOP. The same bug hits
   Plan Review's `followup → revise` transition. This is 100% reproducible and
   still present in the current code.
2. **Protocol fragility (the visible one): the draft model never followed the
   control protocol.** Instead of emitting the ```questions block or
   `READY_TO_PLAN`, it wrote the entire final plan (~10.7k chars, twice) as
   ordinary chat text and ended its turn. It even read
   `prompt_templates/plan/final_plan.md` and role-played the final step. The
   harness's in-band signals (sentinel / questions JSON) are optional,
   text-based, and enforced by at most one nudge — a model that ignores them
   stalls the stage.
3. **A genuine pi crash in the first run.** The 08:54 run died with
   `pi exited -9 after 4 attempts` (SIGKILL mid-stream, while the model was
   emitting `READY_TO_PLAN`). Later logs show the same `rc=-9` with
   `ResourceExhausted: Worker local total request limit reached` from the
   nvidia provider — transient, and the new retry code handles it, but the old
   code turned it into a hard stage error.

The user's proposal (let the model write the plan file itself with write/edit
tools, loop until a tool-less message, verify the file on disk, corrective
retry ×2, then error) is the right direction — it replaces *declared*
completion (sentinel text) with *verified* completion (artifact on disk).
Section "Revamp design" below details it, plus the queue fix that is needed
regardless of any template change.

---

## 1. What actually happened — timeline (all UTC)

### Run 1 — 08:54–08:59 (pre-refactor code, committed as `fee9253` at 10:28)

- 08:54:43 — Plan draft starts, model `cursor/grok-4.5:slow`, session
  `plan/sessions/2026-07-19T08-54-43-305Z_plan-...jsonl`.
- 25 turns of exploration. The agent explores Docker/MCP wiring, fights
  Blender/Wayland/Xvfb for several minutes (turns up to 62s each).
- Last assistant text: `## Hypotheses (what "done" means)…` — no questions
  block, no `READY_TO_PLAN`.
- 08:59:01 — step ends in **error**: `error_step=draft`,
  `error="pi exited -9 after 4 attempts"`. The error_detail tail shows pi was
  SIGKILLed while streaming `READY`,`_`,`TO`,`_`,`PLAN` as text deltas. Sender
  of the SIGKILL is undeterminable (no host OOM records; old container logs
  gone). Same `rc=-9` recurs later with a provider `ResourceExhausted` message,
  so it is environmental, not a code path.
- (The state was committed in this form in `fee9253`; `finished_at` and
  `error_step=draft` in today's `plan.json` are leftovers of this run.)

### Code refactor — 08:59–10:45

- Big refactor committed as `fee9253` ("plan 1 done", 10:28): steprun.py
  (`hop_with_nudge`), `record_prompt` trajectory entries, B1 exclusive enqueue,
  STOP support, tests. Container recreated 10:45:06.

### Run 2 — 10:52–11:03 (current code; this is the stagnation in the chat scrape)

- 10:52:58 — user types **"cool, continue"** into the message form (accepted
  from `error` phase). `user_message` job `1784458377850_7a3e8af2` claimed.
- Hop 1 (5 turns): model re-reads files, reads `prompt_templates/plan/final_plan.md`,
  then writes **the full final plan as chat text** (`# Plan`, 10.7k chars) and
  ends the turn — `reason=agent_end`, no questions, no sentinel.
- 10:54:13 — harness sends the single flow-control nudge ("Stop calling tools
  now… emit ```questions or READY_TO_PLAN").
- Hop 2: model writes only `## Lay of the land` (2.1k) — again **no questions,
  no sentinel**, `reason=agent_end`.
- `_run_draft_step` fallthrough: `READY_SENTINEL in text or rnd >= MAX_ROUNDS
  or not questions` → `not questions` is True → `_to_final` sets
  `phase=final_running` and enqueues `final`…
- **10:54:42.807 — `queue: dropping duplicate plan/final for
  1784450256833_blender_mcp (job 1784458377850_7a3e8af2 already processing)`**
  → the final step never runs. `grep -c "plan-final"` over all logs = 0.
- Stage now shows "Writing final plan…" with nothing behind it. User waits
  ~6 min, changes part models to `nvidia/nemotron-3-ultra-550b-a55b`.
- 11:01:16 — **STOP #1**: `plan: stop requested … (killed=False)` — nothing to
  kill (no pi process; `run_pi_text` writes no pid file anyway). Phase → `stopped`.
- 11:01:25 — user types **"continue writing the plan draft"** (accepted from
  `stopped`). New `user_message` job.
- Hop 1: model writes the full plan **again** (`# Plan`, 10.9k) — agent_end.
- Hop 2 (nudge): first attempt comes back with an **empty assistant message**
  (`content: []`, `ResourceExhausted: Worker local total request limit reached
  (139/32)`, rc=-9) → transient retry after 20s → second attempt:
  "I've already gathered sufficient information… READY_TO_PLAN" — the sentinel,
  at last, buried in prose.
- `READY_SENTINEL in text` → `_to_final` → **11:02:18.553 — `queue: dropping
  duplicate plan/final … (job 1784458885856_55bd7271 already processing)`** —
  dropped again.
- 11:03:14 — **STOP #2** (`killed=False`). Final state: `phase=stopped`,
  `stop_requested=true`, 37 turns, no `final_plan.md` on disk (only
  `plan/draft.md`), Plan Review tab disabled. This is the state the UI shows
  today.

### Model-behavior observations from the trail

- The draft model **did the final step's job inside the draft step**: it read
  `prompt_templates/plan/final_plan.md` and produced the full `# Plan`
  structure as *chat text*, because `draft.md` forbids writing files and gives
  it no artifact to produce. Its thinking literally says "I will write the
  final implementation plan following the exact structure."
- It never asked a single clarifying question. `draft.md` makes questions
  optional ("only ask questions whose answers would materially change the
  plan"), and the model decided none were needed — but it also failed to emit
  the required `READY_TO_PLAN` instead, on four consecutive opportunities
  (initial hop, user message, two nudges).
- `plan/draft.md` on disk (mtime 14:02 local) is the stripped chat text —
  i.e. the "artefact" the harness saved is the model's chat dump, not a
  deliberately written document.

---

## 2. Root causes

### RC1 — `enqueue_exclusive` drops a stage's own next step (critical, current)

`.pi/crack/server/src/crack_server/queue.py:68-85` drops any enqueue whose
`(task_id, slug)` matches a job in `pending/` **or `processing/`**, without
comparing `step`. But `worker._dispatch` (`.pi/crack/server/src/crack_server/worker.py:56`)
only removes the processing file *after* `stage.dispatch_step(...)` returns.
So any step that enqueues its own successor from inside the worker is always
dropped:

- `stages/s02_plan.py:285` — `_run_draft_step` → `enqueue_step(task_id, "final")`
  → **always dropped**. Plan can never auto-advance to the final plan.
- `stages/s03_plan_review.py:383` — `followup` → `enqueue_step(task_id, "revise")`
  → **always dropped**. Plan Review can never auto-advance from Q&A to revising;
  it sticks in `revising` phase with no job.

Both leave the stage in a running phase with no live job: infinite spinner,
no error, recoverable only by STOP. Proof in production logs (twice):
`queue: dropping duplicate plan/final … already processing`.

The behavior is enshrined by `tests/test_plan41.py:284-297`
(`test_enqueue_exclusive_drops_duplicates` asserts a same-slug enqueue while
one is claimed returns `None`) — the guard was designed for double-click
protection (B1) but is over-broad: it can't distinguish "user pressed Plan
twice" from "the running step is chaining to its successor".

Cross-stage transitions are unaffected (different slug): explore→plan,
plan→plan_review (`s02_plan.py:355`), review→implementation, impl→impl_review
all work. Web-process enqueues (submit answers, reject, grill, retry,
user message) are unaffected because by then the previous job has completed.
That is why the earlier long Q&A sessions (the "review round 60" commits)
worked — every round there was user-driven.

Not covered by tests: no test drives a stage through the real
claim → dispatch → complete worker cycle into a self-enqueued successor step.

### RC2 — Completion is signaled by in-band text, not verified artifacts

- The draft protocol (```questions block / `READY_TO_PLAN`) lives entirely in
  model output. The hop runner (`pi_proc.run_agent_hop`) is even called with
  `sentinel=None` for draft hops, so nothing stops the model mid-stream; the
  harness scans the combined text *after* the fact.
- `steprun.hop_with_nudge` sends exactly **one** nudge. A model that answers
  the nudge with more prose (observed: `## Lay of the land`, no sentinel)
  gets no further correction.
- The fallthrough default is wrong: in `s02_plan.py:271`,
  `READY_SENTINEL in text or rnd >= MAX_ROUNDS or not questions` — when the
  model emits *neither* questions *nor* sentinel, `not questions` makes the
  stage advance to final anyway (with only a log warning). Protocol failure is
  treated as success.
- `draft.md`'s read-only constraint ("NEVER write, edit, or create any files")
  leaves the model with no concrete deliverable; models strongly default to
  "produce the document as my message". The model then believes it is done
  ("I've already gathered sufficient information and provided the Lay of the
  Land and full plan") while the harness waits for a control token it never
  sends. This is exactly the reported "model thinks it is done, harness
  regards this as not done".
- The final plan is then regenerated from scratch by a *different* single-shot
  no-tools call (`_run_final` → `run_pi_text`, `PI_TIMEOUT_SECONDS=120`) from
  the chat transcript — lossy, wasteful, and timeout-prone for 10k+ char
  plans. The harness also appends a `READ_ONLY_REMINDER` paragraph into the
  user's plan document (`s02_plan.py:335-336`) — vestigial.

### RC3 — Plan Review's revise step has the same shape of fragility

- `revise`/`reject` already use the file-editing approach the user proposes
  (`tools="bash,read,edit,write,mcp"`, template says "Edit the plan file in
  place at {plan_path}, emit PLAN_REVISED") — good.
- But it runs through the same one-hop-plus-one-nudge `hop_with_nudge`: a
  critic that ends its turn mid-edit (agent_end without sentinel) is not
  continued.
- If `PLAN_REVISED` is missing, the code logs a warning and **proceeds anyway**
  (`s03_plan_review.py:444-447`): it re-reads `final_plan.md` (possibly
  unchanged), regenerates the todo, and moves to `awaiting_approval`. No check
  that the file was actually modified.
- And per RC1, the `followup → revise` transition never fires automatically at
  all today.

### RC4 — Environmental pi kills (rc=-9)

`pi exited -9` appears at 08:59 (hard error under old code) and at 11:01:49
(provider `ResourceExhausted`, handled by the transient-retry path added in
`fee9253`, succeeded on reattempt). Worth knowing the current retry machinery
works; nothing to fix beyond noting that a SIGKILL mid-stream can destroy the
last turn (the 08:59 run lost the `READY_TO_PLAN` it was emitting).

### RC5 — STOP cannot kill single-shot text calls

`request_stop` kills only the pid file written by `run_agent_hop`
(`agent_hop_kwargs`). `_run_final`'s `run_pi_text` and `regenerate_todo`'s
`run_pi_text` write no pid file (logs: `killed=False`). A STOP during
`final_running` sets `phase=stopped`, but the pi process would keep running and
`_run_final`'s `_finish` would later flip the stage to `done`, overwriting the
user's stop. Latent race; masked today only because RC1 prevents `final` from
running.

### RC6 — No reconciliation between phase and queue

Nothing detects "stage in a running phase with no pending/processing job".
`queue.reclaim_orphans` only runs at worker boot and only covers
processing→pending. A periodic (or status-poll-time) check would have turned
RC1's silent spinner into a visible error immediately.

---

## 3. Assessment of the proposed design

The proposal: in Plan (and identically in Plan Review), give the model the
full path of the plan file and have it write the file itself — `write` to
(over)write, `edit` to change sections — across as many turns as it needs; the
harness stops the loop when the model returns a message with no tool calls
(`agent_end`); if the expected files don't exist (or weren't changed), send a
corrective message and take another turn, max 2 tries, then give up with
error.

This is sound, and strictly better than the current design, because:

- **Completion becomes verifiable.** File-exists / file-changed / file-passes-
  a-structure-check is objective; `READY_TO_PLAN` in prose is not. The harness
  stops trusting the model's word and starts checking its work.
- **It matches how these models actually behave.** The trace shows the model
  *wanted* to produce the plan document; `draft.md` just forbade the only
  sensible outlet (a file) so it dumped 10.9k chars into chat. Giving it
  write/edit tools channels the behavior instead of fighting it.
- **It removes the double generation.** Today the draft agent writes the plan
  as chat text and `_run_final` regenerates it single-shot from the transcript.
  One agent writing one file replaces both.
- **It unifies Plan and Plan Review.** Review's revise already works this way
  (edit `final_plan.md` in place); making Plan's output the same kind of
  artifact means review can diff, edit, and iterate on a stable document.

Caveats / refinements needed to make it robust:

1. **RC1 must be fixed first** — otherwise the write phase, however designed,
   is never reached automatically (same dropped-enqueue trap if it is a
   separate step).
2. **`agent_end` alone is not a sufficient stop condition** — keep it, but
   treat it as "settled, now verify". The observed run had *five* agent_end
   events, most premature. The verification (files exist, fresh, structurally
   valid) is what makes settling meaningful. Also keep a wall-clock cap
   (`hop_loop` already has one) so a model that edits forever is cut off —
   and on time cap, still verify artifacts before declaring error.
3. **Freshness matters**: verify the file's mtime/content-hash changed *during
   this step*, not just that it exists — `start()` currently does not clear old
   plan artefacts, so a stale `final_plan.md` from a previous run would satisfy
   a naive existence check.
4. **Validate structure, not just presence**: check the required section
   headings from the template (`# Plan`, `## Changes`, `## Automatic
   verification`, …). Cheap regex; catches "wrote two paragraphs and settled".
5. **The corrective retry should name the deficiency** ("file X is missing /
   unchanged / lacks section Y — produce it now using the write/edit tools")
   and run in the same session so the model keeps context. 2 tries, then
   `error` phase with a clear message — exactly as proposed.
6. **Keep the sentinel as a secondary signal, enforced in-stream.**
   `run_agent_hop` already supports `sentinel=` matched on its own line
   mid-stream (s04 uses `IMPLEMENTATION_COMPLETE`). Passing e.g.
   `PLAN_WRITTEN` there terminates the hop immediately instead of after a full
   trailing message. But never let the sentinel substitute for the file check.
7. **Decide the Q&A policy explicitly.** The user expected the draft agent to
   ask questions after exploring; the model legally declined. Options:
   (a) require ≥1 round like `plan_review/critique.md` does ("always emit at
   least one round of questions"); (b) keep optional but make the no-questions
   path explicit (`READY_TO_PLAN` on its own line, nothing else). The mixed
   "optional but expected" wording in `draft.md` is the worst of both.
8. **Timeout budget**: `DRAFT_TIMEOUT_SECONDS=300` is per *step* and shared
   across hops (`start` is taken once per `_run_draft_step`). A write phase
   that legitimately needs many edit turns needs its own, larger budget
   (s04 uses 3600s).
9. **Interview phase should stay read-only** — but its output contract should
   be only: questions block *or* `READY_TO_PLAN` (plus optionally updating the
   plan file's "Lay of the land" section directly, if we want its notes
   durable; simplest is to keep notes in the session and let the write phase
   re-derive them).

---

## 4. Revamp design (concrete)

### 4.1 Queue/chaining fix (prerequisite, independent of templates)

Pick one:

- **(a) Deferred successor enqueue (recommended).** Let `run_step` return an
  optional next `(step, form)`; `worker._dispatch` calls `queue.complete(job)`
  first, then enqueues the successor. Removes the self-duplicate class of bugs
  entirely; keeps B1 double-click protection intact.
- (b) `enqueue_exclusive(..., ignore_job_id=current)` — the worker passes the
  in-flight job id through `dispatch_step`; the guard skips it. Smaller diff,
  but every stage keeps chaining implicitly and the footgun remains for future
  code.
- (c) Complete-then-run: move `queue.complete` before `dispatch_step`. Breaks
  crash recovery (`reclaim_orphans` relies on the processing file outliving
  the run). Do not do this.

Add a regression test that drives draft→final through the real
claim/dispatch/complete cycle with `fake_pi.sh` (none exists today).

### 4.2 Plan stage flow (s02)

1. **Interview (read-only, tools `bash,read,mcp`)** — mostly today's draft.md,
   tightened: output contract is exactly one of ```questions block or
   `READY_TO_PLAN` alone on the last line. Keep `hop_with_nudge` but change the
   fallthrough: after the nudge, *neither* signal ⇒ one more nudge (max 2),
   then `error` ("planner did not produce questions or READY_TO_PLAN") —
   never silently advance. Optionally require ≥1 question round (policy
   decision, §3.7).
2. **Write (tools `bash,read,edit,write,mcp`)** — new template
   `plan/write_plan.md` with `{plan_path}` (absolute path to
   `tasks/<id>/plan/final_plan.md`), `{content}`, `{explore_summary}`,
   `{qa}`, the required structure (today's `final_plan.md` body), and explicit
   tool guidance: create/overwrite with `write`, revise sections with `edit`;
   when done, reply with a short summary and no tool calls.
   Driver: loop hops (same session) until `agent_end`; then verify:
   file exists, mtime within this step, contains the required headings.
   Missing/invalid → corrective message, up to 2 retries → else `error`.
   This replaces `_run_final`/`run_pi_text` entirely (delete the single-shot
   final, the `READ_ONLY_REMINDER` append, and the separate `final` part —
   or keep a cheap no-tools *polish* step if desired, but it is redundant).
3. `start()` must clear stale plan artefacts (`draft.md`, `final_plan.md`,
   round files) along with the sessions dir, or the freshness check in (2)
   can pass on an old file.

### 4.3 Plan Review flow (s03)

- Critique Q&A stays as-is (it already mandates ≥1 round).
- Fix the `followup → revise` chain via 4.1.
- `revise`/`reject`: keep file editing + `{plan_path}`, but drive with the same
  settle-then-verify loop: hop until `agent_end`, then verify `final_plan.md`
  *changed* (content hash before vs after) — not merely that `PLAN_REVISED`
  appeared in text; on no-change, corrective retry ×2, then error. Keep
  `PLAN_REVISED` only as an in-stream early-termination sentinel via
  `run_agent_hop(sentinel=...)`.
- Regenerate todo only after verification passes.

### 4.4 Shared machinery

- New steprun helper, e.g. `run_until_settled(run_hop, verify, corrective,
  max_corrective=2, timeout_seconds)`, implementing settle → verify →
  corrective-retry; s02-write and s03-revise share it. `hop_loop`/`hop_with_nudge`
  stay for the interview stages.
- STOP parity for `run_pi_text` (pid file) or migrate remaining text calls
  that matter into hop-shaped runs — fixes RC5.
- Orphan-phase watchdog: on status render or a periodic worker sweep, if a
  stage is in a running phase with no matching pending/processing job, mark it
  `error` ("no live job for running phase — likely dropped enqueue") — turns
  RC1-class regressions into visible errors instead of infinite spinners (RC6).

### 4.5 Template changes summary

- `plan/draft.md` — tighten output contract (questions XOR `READY_TO_PLAN`);
  decide mandatory-questions policy; keep read-only.
- `plan/draft_followup.md` — same contract restated (already close).
- **new `plan/write_plan.md`** — file-writing instructions + `{plan_path}` +
  structure + "finish with a tool-less message".
- `plan/final_plan.md` — delete (or repurpose as the structure spec embedded
  in `write_plan.md`).
- `plan_review/revise.md`, `plan_review/reject.md` — keep; adjust ending
  instruction to "finish with a short summary and no further tool calls" (+
  optional `PLAN_REVISED` line for early termination).
- `plan_review/critique.md`, `grill_followup.md`, `todo.md` — unchanged.

---

## 5. Evidence index

- `queue: dropping duplicate plan/final … already processing` — docker logs
  10:54:42.807 and 11:02:18.553 UTC.
- `queue.enqueue_exclusive` — `.pi/crack/server/src/crack_server/queue.py:68-85`.
- Worker completes job after dispatch — `.pi/crack/server/src/crack_server/worker.py:56`.
- Self-enqueue sites — `stages/s02_plan.py:285`, `stages/s03_plan_review.py:383`.
- Test enshrining the drop — `.pi/crack/server/tests/test_plan41.py:284-297`.
- Run-1 error — `git show fee9253:.pi/crack/tasks/1784450256833_blender_mcp/plan.json`
  (`pi exited -9 after 4 attempts`, error_detail shows SIGKILL mid-`READY_TO_PLAN`).
- Empty-turn / ResourceExhausted retry — session jsonl message `c1d12f11`
  (11:01:49, `content: []`) + docker log transient-reattempt lines.
- STOP with `killed=False` — docker logs 11:01:16 and 11:03:14;
  `stages/base.py:230-248` (`request_stop` kills only the agent pid file).
- Current stuck state — `plan.json` (`phase=stopped`, 37 turns, no
  `final_plan.md`) and the live UI (Re-plan button + message form, Plan Review
  tab disabled).
