# crack-server: possible bugs (beyond the ones already reported)

Scope: `.pi/crack/server/src/crack_server/`. These are bugs found by reading the whole
implementation that are **not** already covered by `pi-crack-server-1-prompt-1.md`,
`pi-crack-server-2-plan-1.md`, or the new problems listed in `pi-crack-server-3-prompt-2.md`
(prompt visibility, polling reset/spam, duplication). Each has file:line refs against the
current tree. B-numbers are referenced from the `pi-crack-server-4-plan-{1,2,3}.md` files.

---

## Concurrency / lifecycle (high)

**B1 — Double-start race on every stage.** `Stage.start` implementations check
`state.get("status"/"phase") == running` then write state and enqueue
(`s01_explore.py:202-230`, `s02_plan.py:94-125`, `s03_plan_review.py:75-104`,
`s04_implementation.py:125-143`, `s05_impl_review.py:69-86`). Two near-simultaneous POSTs
(double-click, htmx retry) both read the pre-running state and both enqueue → two worker
threads run the same stage with the same pi `--session-id` and interleaved
read-modify-write of the same JSON state. There is no per-(task, slug) claim anywhere;
the worker happily runs both (`worker.py:58-75`).

**B2 — Concurrent sends into a running chat.** `chats.post_message` (`chats.py:230-248`)
and `S06Finished.handle_action("chat")` (`s06_finished.py:60-72`) append an exchange and
enqueue even when `phase == "chatting"`. Result: two `run_chat`/`_run_chat` jobs run
concurrently against the *same* pi session dir, and both `persist` closures captured
different `idx`/`existing` snapshots, so one overwrites the other's turns
(`chats.py:355-367`, `s06_finished.py:94-106`).

**B3 — Lost updates on shared JSON state.** All state mutation is
read-modify-write-whole-file with no lock: e.g. worker `_persist_explore_turn`
(`s01_explore.py:80-96`) races the web process writing the same file (stop flags, title
saves, `submit_answers`). `_atomic_write_json` (`paths.py:121-126`) makes each write
atomic but not the read-modify-write cycle, so a concurrent writer's fields are silently
reverted. Concrete case: `stop_chat` sets `stop_requested` (`chats.py:258-260`) while the
worker's `persist` is mid-cycle — the next `write_chat_state` from `persist` re-writes
state from a pre-stop snapshot and the stop flag is lost until the next `stop_check` read.

**B4 — watchfiles restart orphans running pi subprocesses, then reruns the job.**
`worker.main` runs `_loop` under `watchfiles.run_process` (`worker.py:80-86`). On any
source edit the worker process is killed; pi subprocesses survive (they are in their own
session per `start_new_session=True`, `pi_runner.py:563-569`). The restarted worker's
`reclaim_orphans` (`queue.py:111-131`, threshold 5s) requeues the in-flight job, so a
*second* pi now runs against the same `--session-id`/session dir while the orphan is
still streaming. Nothing kills the orphan (its pid_file, when one exists, is not consulted
on reclaim).

**B5 — Replayed jobs are not idempotent.** After a reclaim (B4) or worker crash, the
requeued step re-executes from its beginning: explore's `"run"` re-runs turn-zero and
sigmap and re-sends the full hop-1 prompt into a session that already has turns
(`s01_explore.py:258-308`); plan's `"draft"` re-sends `draft.md` similarly. Turn lists in
state keep appending, so the trajectory shows duplicated question rounds/prompts.

**B6 — Worker-level failure leaves a stage stuck "running" forever.** `_dispatch` calls
`queue.fail(job)` which just deletes the job file (`worker.py:52-55`, `queue.py:105-108`).
Any exception that escapes the stage's own try/except — e.g. the default
`run_step` `NotImplementedError` for a misrouted step (`base.py:131-136`), an exception
raised inside a stage's *except* block, or an OOM-killed thread — leaves the stage state
in a running phase with no job in the queue. The UI spins forever; there is no watchdog
and no "flip to error on dispatch failure".

**B7 — Deleting a task/chat while its job runs resurrects it.** `api_delete_task`
(`app.py:527-545`) and `delete_chat` (`chats.py:271-282`) remove the directory but do not
purge that task's pending/processing queue jobs, and (for tasks) do not kill a running pi.
The worker's next `write_*_state`/`persist` recreates the directory via
`mkdir(parents=True)` (`paths.py:121-126`), leaving a zombie task/chat dir containing only
a state JSON. `delete_chat` at least kills the pid; task delete kills nothing.

## Runner / streaming (medium)

**B8 — Sentinel matching is substring-based over accumulated turn text.**
`run_agent_hop` stops as soon as `sentinel in current_turn["text"]`
(`pi_runner.py:645-656`). If the model *mentions* the sentinel ("I will emit
EXPLORATION_COMPLETE once…") rather than emitting it on its own line, the hop terminates
early. The prompts even quote the sentinel back to the model in follow-up messages
(`s01_explore.py:348-353`), increasing echo probability.

**B9 — Per-step turn budget resets in plan/critic loops.** `s02` passes
`total_turns=count_turn_groups(turns)` where `turns` is only *this step's* list
(`s02_plan.py:217`), and s03 similarly (with `existing_turns + new_turns`,
`s03_plan_review.py:241`, correct) — s02 ignores `existing_turns`, so `DRAFT_MAX_TURNS`
is effectively per-answer-round, not per-stage. Inconsistent with s03/s04/s05. (Moot once
turn caps are removed, but shows the cap plumbing is inconsistent today.)

**B10 — `_looks_failed` marker list is far too eager.** `s04` switches to the fallback
model when two adjacent turns have a failing tool signature, but the markers include bare
`"error"`, `"not found"`, `"failed"`, `"cannot"` (`s04_implementation.py:43-55`). A `rg`
over code that *contains* the word "error" (this codebase logs errors everywhere), or a
grep listing `"not found"` in test fixtures, matches. Combined with
`len(all_turns) > 10` (raw turns, not groups — docstring says turns, code counts list
length, `s04_implementation.py:255-258`), the primary model is nearly always demoted.

**B11 — `_gate_reply_is_junk` false positives end exploration.** Any gate reply
containing `"</"` (i.e. any HTML/XML snippet, closing markdown-code tags) is classified
junk → treated as DONE (`s01_explore.py:124-132`, `342-347`), silently ending exploration
early.

**B12 — Time-cap check only fires when events arrive.** The `timeout_seconds` check
lives inside the `for line in proc.stdout` loop (`pi_runner.py:691-697`). If pi hangs
without emitting output, the loop blocks on the pipe read indefinitely — there is no
watchdog and `PI_TIMEOUT_SECONDS`-style `subprocess.run` timeouts don't apply to the
streaming Popen. A hung MCP server means a hung stage until manually killed.

**B13 — Rolling `output_tail` micro-bug.** `output_tail.append(...)` then
`del output_tail[:-OUTPUT_TAIL_LINES]` (`pi_runner.py:605-606`) keeps the last 10 *JSON
event* lines, which for a crash right after startup usually contains only well-formed
events, not the stderr that explains the crash — because stderr is merged into stdout and
consumed by the JSON parser path, non-JSON lines are logged at WARN but truncated to 200
chars (`pi_runner.py:611`). Diagnosable, but the UI "last output" detail is frequently
useless noise.

## State / UI-facing (medium)

**B14 — Stale error card on the Finished tab never clears.** `S06Finished.handle_action`
("chat") does not `pop("error")`/`pop("error_detail")` before enqueueing
(`s06_finished.py:60-72`), and `_run_chat`'s success path doesn't clear a pre-existing
error either (`s06_finished.py:133-138`). Once one exchange errors, the error card renders
above the form for every later (successful) exchange (`s06_finished.py:203-204`).
`chats.post_message` gets this right (`chats.py:243-245`) — the two code paths diverged.

**B15 — Title regen result clobbers a manually edited title.** The GET poll
`title_regen_status` auto-saves the generated title when it observes `done`
(`app.py:668-696`). If the user edits and saves the title while a regen job is running,
the poller overwrites their edit whenever the job lands. GET also has side effects (writes
info.json + regen state), so two open tabs double-fire it.

**B16 — `_start_title_regen_job` has a status race and no orphan recovery.** Check
`status == "running"` then write running + enqueue (`app.py:403-418`): two rapid prompt
saves enqueue two title jobs (last-writer-wins on the result — mostly benign). Worse: if
the worker dies mid-job (B6), `title_regen.json` stays `"running"` forever and every later
save silently refuses to start a new regen, with the header stuck on "generating title…".

**B17 — Task id collisions within the same millisecond.** `generate_task_id`
(`paths.py:208-210`) has no uniquifier (unlike `generate_chat_id`,
`paths.py:564-572`) — two creates in the same ms with the same title get a 400
"task already exists" from `create_task`.

**B18 — `read_info` fabricates fresh timestamps for missing/corrupt info.json.**
(`paths.py:102-109`) Any render of a broken task shows "created just now", and
`_render_task_card`/`task_page` can't tell a real task from a directory with no
info.json (e.g. one resurrected by B7).

**B19 — `stage_view` renders pages for nonexistent tasks.** `_check_task_id`
(`app.py:451-455`) validates only the id *format*; `/tasks/<id>` and
`/tasks/<id>/view/<slug>` for an id that doesn't exist on disk render a fully populated
empty task page (default info, idle stages) instead of 404 — and any interaction with it
(add prompt, start stage) creates the directory.

## Performance / operational (low-medium)

**B20 — State JSON grows unbounded and is rewritten per turn.** Every persisted turn
re-reads and rewrites the whole state file including all prior turns with full tool
outputs (`s01_explore.py:80-96` and the four sibling persist closures). Long
implementation runs make this O(n²) disk I/O, and every 1.5s poll re-parses the same
multi-MB JSON and re-renders it to HTML (including `_render_path_ref` re-reading every
referenced file from disk on each poll, `s01_explore.py:54-72`, `469-474`).

**B21 — `models_mod.get_models()` can run a 60s subprocess inside page render.** The
model dropdowns call `get_models()` during rendering (`base.py:209-215`,
`chats.py:138-140`); on a cold/stale cache this shells out to `pi --list-models` with a
60s timeout (`models.py:26-51`) — a page load or chat-fragment poll can hang up to a
minute. Cache refresh belongs on the worker or a background thread, never in render.

**B22 — Retry backoff sleeps occupy worker slots.** A failing pi call holds its worker
thread for the full 61s retry window (`pi_runner.py:69-79` called from the retry loops)
— with `MAX_WORKERS = 4` (`worker.py:25`), four simultaneously-retrying jobs starve the
whole queue, including instant jobs like title regen.

**B23 — pi session dirs accumulate forever.** Session JSONL under
`…/sessions/` is never pruned except by the `rmtree` on stage restart; unscripted chats
never prune at all. Disk-growth only, but worth a janitor.

**B24 — `CHAT_ID_RE` hard-codes 13-digit ms epochs** (`paths.py:15`) — also used to
*list* chats (`paths.py:558-561`), so any future id format change silently hides existing
chats. Trivial, but it bit similar code before (task ids use a different scheme).

## Security-ish (low, local tool)

**B25 — Prompt/template/plan artefact writes accept any `*.md` basename and are
web-editable without auth.** By design for a local dev tool, but `write_stage_template`
(`paths.py:351-355`) writes into the *server package* directory from a web form — a
stray `0.0.0.0` bind (`main.py:9`, default host is `0.0.0.0`!) exposes template-file
writes (and via templates, arbitrary prompt injection into agents that run with
`bash`/`edit`/`write` tools) to the LAN. Note the README claims the default is
`127.0.0.1` but `main.py` defaults to `0.0.0.0` — the code and docs disagree; binding
localhost by default is the safer fix.
