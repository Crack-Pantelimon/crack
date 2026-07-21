# Async worker + suspend semantics: `wait_join` and `ask_user`

## Context

Parent pi agents spawn sub-agents via `spawn_<persona>` tools and then burn tokens hand-rolling `sleep 30 && cat report.md` bash polling loops, because the spawn tool only promises "results will arrive". We want (1) a `wait_join` tool that suspends token-free until sub-agent(s) finish and returns their reports as the tool result, and (2) an `ask_user` tool that suspends a session indefinitely until the user answers, with timeouts reset.

**Decisions locked with the user:**
- **Merge the worker into the FastAPI server as one asyncio process** — true in-memory signaling (futures/events), no cross-process gap for user answers.
- **Full async conversion** of the dispatch chain (worker, pi_proc, chats, chat_engine, sub_agents/*) in one pass.
- **`ask_user` is hop-terminating** (generalizes the planner's `awaiting_answers` pattern): the hop ends cleanly, no pi process is held for hours; a fresh session-resume hop delivers the answer. **`wait_join` is a blocking tool** (bounded minutes-scale waits; results land as the tool result via server long-poll).
- **Disk is truth, memory is cache**: all suspend state mirrored to `run.json`/chat state; asyncio events are pure wakeups; restart recovery via existing `reclaim_orphans` + session-resume.

**Verified architecture facts:**
- The thread cap (24) exists only because hops block threads on `subprocess.run` — LLM calls and bash already run inside the `pi` subprocesses. Async makes *waiting free*; the cap and starvation both disappear.
- Worker already runs under `watchfiles.run_process` ([worker.py:301-309](/.pi/crack/server/src/crack_server/worker.py#L301-L309)) mirroring uvicorn reload — merging doesn't worsen dev-reload; `reclaim_orphans` + `_kill_orphaned_agents` already handle restart.
- `queue.enqueue_exclusive` dedupes against `pending/`+`processing/` ([queue.py:89-136](/.pi/crack/server/src/crack_server/queue.py#L89-L136)) → the push-drain job is dropped while a parent is mid-hop; `resume.drain_children` and `chats._merge_child_inbox` no-op on empty inbox. So wait_join's inbox-drain dedup is nearly free.
- Race: `finish()` sets `parent_notified=True` in one write, appends the inbox entry in a later write — "notified but no entry" is transient (two-strike rule below).
- Chats have no `children` list — enumerate via `paths.list_run_ids(chat_id)` filtering `parent_kind/parent_id`.

## Phase A — one asyncio process

**Files:** `app.py`, `worker.py`, `pi_proc.py`, `chats.py`, `chat_engine.py`, `sub_agents/{base,resume,runner,planner}.py`, `queue.py` (minor), tests.

1. **app.py lifespan** starts `worker.async_loop()` (+ orphan sweep task) as asyncio tasks; graceful shutdown cancels them. Retire the separate worker console-script (keep `main()` as a deprecation stub or delete; update whatever launches the two processes to launch one). Verify the watchfiles/uvicorn reload filter watches only `src/` — data-dir writes (`run.json`, queue files) must not trigger reloads.
2. **worker.py**: poll loop → async task spawner (`asyncio.create_task(_dispatch(job))` per claimed job). Drop ThreadPoolExecutor and the 24 cap entirely — no semaphore on hops (a capped semaphore would be held by wait_join-blocked parents and reintroduce starvation); the existing pi rate limiter is the LLM-pressure guard. Add an in-memory `asyncio.Event` wakeup set by `queue.enqueue*` so dispatch latency drops below the 0.5s poll (poll stays as fallback/truth).
3. **pi_proc.py** (the careful rewrite, 584 lines): `subprocess.Popen` → `asyncio.create_subprocess_exec`; `_stream_events` line loop → `async for` on stdout; `threading.Timer` watchdog → `asyncio` timeout task. **Exact parity required** for: own-session/process-group start, `os.killpg` kill paths, pid-file write/cleanup, STOP classification, retry/backoff (4x/61s), output tails.
4. **Async propagation** (mechanical): `chats.run_chat`/`run_exchange` path, `chat_engine.run_exchange`, `base.dispatch_step/_run_hop/_begin_run`, `resume.drain_children`, planner steps, `runner.finish` callers. `JsonState`/queue file I/O stays sync inline (fast); check flock acquisition paths can't block the loop (wrap in `asyncio.to_thread` only if a blocking flock exists).
5. **Tests**: migrate harness to async (pytest-asyncio/anyio); `_drain_jobs` becomes `async`, dispatching via `await worker._dispatch(...)`. This is the biggest churn — test_plan41, test_sub_agents, and friends.

## Phase B — signals + `wait_join`

**New:** `sub_agents/signals.py` — in-process registry `{(parent_kind, parent_id): asyncio.Event}`; `notify_parent(...)` called by `runner.finish()` *after* the inbox write. Events carry no payload — waiters always re-read disk.

**New:** `sub_agents/wait.py`:
- `_direct_children(chat_id, parent_kind, parent_id)` — run parent: state's `children` + inbox run_ids; chat parent: scan run.jsons. Descriptors `{run_id, persona, phase, notified}`.
- `drain_matching(...)` — one atomic `JsonState.update` partitioning `child_inbox` (matching → returned, rest kept). Single consumption point.
- `poll(...)` — resolve `target` (`all`/run_id/persona-slug; slug or id resolving only to already-notified runs → immediate `delivered_earlier` results via new `runner.build_entry()` factored out of `finish()`), drain, format with `format_child_result`; `rebuild=[ids]` returns entry-rebuilt results without an inbox entry.

**Route:** `POST /api/chats/{chat_id}/sub_agents/wait` — body `{parent_kind, parent_id, target?, run_ids?, rebuild?, block_seconds?}`. Long-poll: run `poll()`; if targets unresolved and `block_seconds` (cap ~25s), `await` the parent's signal event with timeout, re-run `poll()`. Costs nothing in the merged asyncio server. While a waiter is registered, stamp `waiting_on`/`waiting_since` into the parent run state (cleared on return) — orphan sweep skips it, and the pi hop watchdog **extends its deadline while `waiting_on` is set** (timeouts count active time only).

**Extension tool** in [.pi/extensions/crack/index.ts](/.pi/extensions/crack/index.ts): `wait_join {target?, timeout_seconds?}`, `executionMode: "parallel"`. Handler: env checks as spawn; `timeout = clamp(?? 600, 5, 3600)` (active-time exclusion makes long waits safe); loop issuing 25s long-polls, accumulate results, honor abort signal, ~30s transient-fetch-failure tolerance. First poll with no targets/results → ~10s grace re-resolve, then "No outstanding sub-agents". **Two-strike rule:** a target `notified: true` with no inbox entry on two consecutive polls → next poll requests `rebuild` (marked `delivered_earlier`). Timeout → "Still running: {id (persona, phase)}… call wait_join again (free) or end your turn — results arrive automatically." Also update spawn tools' success text: mention wait_join, forbid report-file polling.

## Phase C — `ask_user` (hop-terminating)

1. **Extension tool** `ask_user {question: string, choices?: string[]}` — available everywhere. Handler POSTs `/api/chats/{chat_id}/ask_user` with parent ctx; returns "Question recorded. This session suspends until the user answers — end your turn now, make no further tool calls."
2. **Server:** store `pending_question` in run/chat state. For **run** parents: set phase `awaiting_user`; `_after_hop` treats it like planner's `awaiting_answers` (no nudge, no successor); orphan sweep + `SUBAGENT_TIMEOUT` skip the phase (clock resets on resume). For **chat** parents: record the question so the UI can render it prominently; the chat's normal input is the answer channel (no new suspend machinery — chats already idle between exchanges).
3. **Answer flow:** UI form (htmx fragment on run page + chat page, mirroring planner's Q&A rendering) → `POST .../runs/{run_id}/user_answer` → store answer, enqueue a resume step that runs a session-resume hop with the answer as the user message (reuse/generalize planner `submit_answers` machinery in [sub_agents/planner.py](/.pi/crack/server/src/crack_server/sub_agents/planner.py)). Note: planner's own grill flow can migrate onto `ask_user` later — out of scope.

## Prompts

`.pi/crack/sub_agents/{coder,explorer,planner,tester}/system.md`: document `wait_join` (block for results, free while waiting, never bash-poll report files) and `ask_user` (suspend for human input anytime). Tests inherit via `_seed_personas`.

## Tests (additions in `tests/test_sub_agents.py` + new `test_wait_join.py`/`test_ask_user.py`)

- Wait: drains chat inbox + no duplicate `child_report` exchange after held drain job runs; run-parent drain + no duplicate `child_results` hop; target resolution (all/run_id/slug/bogus/delivered_earlier); notified-gap not misread, `rebuild` works; long-poll wakes on `signals.notify_parent` (< block_seconds); route validation.
- ask_user: tool call → `awaiting_user`, no nudge/successor; orphan sweep skips it; answer → resume hop receives answer text; chat-context question recorded.
- Async conversion: existing suites green under the async harness; a concurrency smoke test (two chats' hops genuinely interleave — FakePi with sleeps).

## Risks

- **pi_proc parity** is the regression hotspot (group kills, STOP classification, streaming persistence) — port with the existing tests as the harness before touching anything else.
- **Deployment change**: one process instead of two — update launch scripts/container config; ensure reload watch excludes data dirs or state writes cause reload storms.
- **No hop cap**: unbounded concurrent pi subprocesses; the rate limiter guards LLM pressure, but note a config knob (max concurrent *non-waiting* hops) as a follow-up if needed.
- **Restart mid-wait**: killed pi hops re-run via `reclaim_orphans` and re-enter their wait (disk truth, no wait state lost) — pre-existing semantics, but verify the replayed hop re-issues wait_join sanely (session replay includes the prior tool calls).

## Verification

1. `python -m pytest` in `.pi/crack/server` (full suite under async harness).
2. E2E: launch the merged server; chat spawns two explorers + `wait_join` — trajectory shows one blocking tool call (no bash sleeps), both reports in the tool result, no duplicate `child_report` exchange after.
3. E2E ask_user: sub-agent calls `ask_user`, run shows `awaiting_user` with the question in the UI, no nudges accrue overnight-style (sweep skip), answering resumes the run with the answer.
4. Kill the server mid-`wait_join`, restart: run recovers via reclaim + session-resume, wait completes.
