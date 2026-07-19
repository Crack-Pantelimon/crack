# Plan 4.3 — De-duplication refactor + residual bug fixes

One of three independent plans (4.1 runner/lifecycle, 4.2 web transport/UI, 4.3 this).
This part is behavior-preserving restructuring plus the bug fixes from
`_slop/pi-crack-server-5-next-bugs.md` that neither 4.1 nor 4.2 absorbs. It can be
implemented standalone against the current tree; if 4.1/4.2 land first, the same moves
apply to their versions of the code (the duplication they inherit is the same).
Recommended order overall: 4.1 → 4.2 → 4.3, but nothing here depends on them.

Rules of thumb enforced at the end: **functions < 100 lines, files < 500 lines.**
Current offenders: `app.py` 972, `pi_runner.py` 777 (`run_agent_hop` alone ~290 lines),
`base.py` 676, `paths.py` 623, `s03_plan_review.py` 549 (`_run_review_step` ~140 lines),
`s01_explore.py` 511 (`_run_job` ~150 lines).

---

## Part A — Duplication inventory → target modules

### A1. Generic JSON state store (kills ~200 lines of paths.py; fixes B3 groundwork)

`paths.py` repeats the same read/write pair **nine** times (`title_regen` 129-144,
`explore` 147-162, `models_cache` 285-296, `stage_config` 305-320, `plan` 363-378,
`plan_review` 417-432, `implementation` 464-479, `impl_review` 492-507, `finished`
520-535, `chat_info`/`chat_state` 575-608 — eleven, in fact).

New `state.py`:

```python
class JsonState:
    def __init__(self, path: Path): ...
    def read(self) -> dict            # tolerant: {} on missing/corrupt
    def write(self, data: dict)       # atomic tmp+rename (moved from paths._atomic_write_json)
    def update(self, fn: dict -> dict) -> dict   # read-modify-write under a per-path
                                                 # filelock (fcntl.flock on <path>.lock)
```

- `paths.py` keeps only path *construction* (`explore_path(task_id)`, …) and exposes
  `def explore_state(task_id) -> JsonState` one-liners (or a single
  `def stage_state(task_id, name) -> JsonState`). All ~40 `read_x_state`/`write_x_state`
  call sites across app/stages/chats mechanically become `X_state(id).read()` /
  `.write()`.
- **B3 fix:** every read-modify-write cycle (persist closures, stop handlers,
  `submit_answers`, status flips) goes through `update(fn)` so the web process and
  worker can no longer silently revert each other's fields. `flock` works across the
  two processes; the lock is held only for the read-modify-write, never during pi work.

### A2. Shared stage-step runner (kills the 5 persist closures + 6 error blocks + 4 hop loops)

The identical `persist` closure (turn-dict construction + `existing + new_turns`
write-back) appears at `s02_plan.py:187-199`, `s03_plan_review.py:211-223`,
`s04_implementation.py:222-235`, `s05_impl_review.py:152-165`, `chats.py:355-367`,
`s06_finished.py:94-106`, and near-identically at `s01_explore.py:80-96`. The
`while reason == "hop_cap"` loop is at `s02:201-231`, `s03:225-251`, `chats:369-397`,
`s06:108-131`. The `except Exception → phase=error/error_detail/error_step/finished_at`
block is at `s01:404-412`, `s02:266-274` and `317-325`, `s03:387-395`, `s04:293-301`,
`s05:204-212` (and the chat variants `chats:406-416`, `s06:140-146`).

New `stages/steprun.py`:

- `def make_turn(current_turn, hop) -> dict` — the one turn-dict constructor.
- `def turn_persister(state: JsonState, key="turns", subpath=None)` — returns the
  persist callable; `subpath` covers the chat `exchanges[idx]["turns"]` case.
- `def hop_loop(*, run_hop, max_hops, continue_message, stop_check=None) -> str` — the
  shared while-loop (drop entirely if plan 4.1's cap removal landed; keep the loop
  driver for continue-nudges either way).
- `@stage_step(state_getter, step_name)` decorator (or context manager
  `with record_errors(state, step):`) wrapping every `_run_*` worker method with the
  canonical error-state write. Six stages lose their copy-pasted except blocks.

### A3. Chat engine unification (chats.py vs s06_finished)

`chats.run_chat` (`chats.py:326-416`) and `S06Finished._run_chat`
(`s06_finished.py:80-146`) are the same algorithm with different state files, session
dirs, and toolsets. Extract `chat_engine.py`:
`def run_exchange(*, state: JsonState, sessions_dir, session_id, model, tools, message_builder, log_prefix)`.
Both callers become <20-line adapters. Same for the duplicated
"You:"-bubble + trajectory rendering (`chats.py:170-178` vs `s06_finished.py:193-201`)
→ one `render_exchanges(exchanges)` helper next to the other renderers.

### A4. app.py split (972 → four files < 400 each)

- `ui.py` — `_esc`, `_format_time`, `_format_ago`, `_render_markdown`,
  `_render_base`, and the title-slot renderers (`app.py:41-225`). The six stages'
  `_esc` wrapper shims (`s0N:_esc`) die; they import from `ui`.
  (`base.py:29` imports `app as _ui` to dodge a cycle — importing leaf `ui.py`
  removes the cycle properly.)
- `routes_tasks.py` — task/prompt CRUD + title regen routes (`app.py:463-697`).
- `routes_stages.py` — stage view/status/action/config routes (`app.py:704-918`).
- `routes_chats.py` — the chat routes (`app.py:926-972`).
- `app.py` — FastAPI construction, static mount, `include_router` × 3, ≤ 60 lines.
- Delete the legacy explore/plan-specific routes (`app.py:704-743`) — they duplicate
  the generic `/stages/{slug}/…` routes; grep templates/htmx attrs for usages first
  (`explore-status`, `plan-status`, `/plan/answers`) and repoint to generic URLs.

### A5. base.py split (676 → three files)

- `stages/base.py` — `Part`, `Stage`, `STATUS_COLORS`, action/queue plumbing (~250).
- `stages/qa.py` — `parse_questions`, `collect_answers`, `format_qa_for_prompt`,
  `render_qa_history`, `render_questions_form` (`base.py:326-470`). Also delete the
  duplicate `_QUESTIONS_BLOCK_RE` in `s02_plan.py:52` (import from qa).
- `stages/render.py` — actions-table/trajectory/error/spinner/retry-button renderers
  (`base.py:476-677`) plus prompt-row/template-row shared form (the near-identical
  view/edit article markup in `app.py:328-374` vs `base.py:232-274` becomes one
  `render_file_row(view_url, save_url, name, content, meta, editing)`).

### A6. pi_runner.py split (777 → three files, `run_agent_hop` < 100 lines)

- `ratelimit.py` — `RateLimiter`, limiter registry, retry-offset helpers.
- `pi_proc.py` — `run_pi_text`, `run_agent_hop`, `kill_pid_file`, `PiError`. Break
  `run_agent_hop` up: extract `_stream_events(proc, sink)` (the for-line loop),
  `_TurnAccumulator` (wraps `apply_event_to_turn` + timing dicts + group counting),
  and the retry wrapper — target ≤ 90 lines per function.
- `transcript.py` — `text_from_content`, `apply_event_to_turn`, `count_turn_groups`,
  `truncate_output`, `tail_truncate`, `fit_nano_transcript`,
  `render_transcript_plaintext`, `extract_path_refs`, `resolve_path_ref`,
  `read_file_lines`.
- Keep `pi_runner.py` as a thin re-export shim for one release so stage imports don't
  all churn in the same commit (then inline it).

### A7. Small dedupes

- Model-`<select>` markup duplicated at `base.py:209-230` and `chats.py:136-161` →
  `render.model_select(name, current, post_url)`.
- Title generation duplicated: `app._run_title_regen_worker` (`app.py:421-436`) vs
  `chats._maybe_generate_title` (`chats.py:297-323`) → one `titles.py` helper
  (single place to clamp length — task titles currently aren't clamped, chat titles
  are, B-adjacent).
- The `msg_count` arithmetic per stage (`s01:500`, `s02:346-353`, `s03:530-537`,
  `s04:382`, `s05:288`, `s06:219`) → `Stage._default_msg_count(parts, turns)` with
  overrides only where genuinely different.

---

## Part B — Residual bug fixes (B-refs from the bugs file)

Fixes not covered by 4.1 (which takes B1, B2, B4, B5, B6, B8, B10 partial, B12, B14,
B22-mitigation) or 4.2 (B20 render half):

- **B7** delete-vs-running: `api_delete_task` kills every `tasks/<id>/*.agent.pid`,
  removes that task's jobs from `queue/pending` + `processing` (new
  `queue.purge(task_id)`), then rmtrees. `delete_chat` gains the same `purge`.
  Additionally `JsonState.write` (A1) refuses to create its parent dir when the
  task/chat dir has been deleted (`if not path.parent.parent.exists(): skip+log`) so a
  straggler worker write can't resurrect it.
- **B11** `_gate_reply_is_junk` (`s01_explore.py:124-132`): drop the bare `"</"`
  heuristic; keep `<tool_call`/`<function` and the leading-command regex.
- **B13** keep a separate raw-stderr tail: collect non-JSON lines into their own
  ring buffer and prefer it in `PiError.detail` when nonempty.
- **B15/B16** title regen: regen result only auto-saves when the info.json title is
  unchanged since the job started (store `base_title` in the regen state when
  starting; compare before overwrite). Make the *save* a POST (htmx) instead of a
  side-effecting GET. Add staleness recovery: a `"running"` regen state older than
  10 min is treated as error ("worker lost"), so the header can't stick.
- **B17** `generate_task_id`: reuse the `_n` uniquifier loop from `generate_chat_id`
  (`paths.py:564-572`).
- **B18/B19** `read_info` returns `None` for missing info.json (callers render "broken
  task" or 404); `_check_task_id` gains an `exists=True` mode used by the page/status
  routes so `/tasks/<garbage-but-valid-format>` 404s instead of materializing.
- **B21** `get_models()` at render time: render from cache only
  (`read_models_cache()` or `FALLBACK_MODELS`), and refresh via a queue job
  (`__models__` pseudo-slug, mirroring `TITLE_JOB_SLUG`) enqueued when stale — page
  loads never shell out.
- **B23** janitor: on worker startup, delete session dirs of tasks/chats finished
  more than N days ago (config constant, default 14) — one small function, logged.
- **B24** widen `CHAT_ID_RE` to `^\d{13,}(_\d+)?$`.
- **B25** `main.py:9` default host → `127.0.0.1` (matching the README); keep
  `CRACK_PI_HOST=0.0.0.0` as the explicit opt-in for the container (set it in
  `_docker` scripts, which already publish the port).

---

## Sequencing & verification

1. A1 state store (mechanical; unit-test `JsonState.update` with two processes
   hammering the same file — no lost fields).
2. A2 steprun + A3 chat engine (behavior-preserving: run one full pipeline with the
   fake-pi shim before/after and diff the resulting state JSONs modulo timestamps).
3. A4/A5/A6 file splits (imports only; `uv run python -c "import crack_server.app"`,
   then click through every page).
4. A7 small dedupes.
5. Part B fixes, each with its targeted check from the bugs file.

Final gates:

- `wc -l` every file under `src/crack_server/` — all < 500.
- A crude function-length check:
  `rg -n "^\s*def |^\s*async def" -A0` + reviewer eyeball, or
  `python - <<'EOF'` ast script asserting no function > 100 lines `EOF` — run it in CI
  fashion once and fix stragglers.
- `rg "read_(explore|plan|plan_review|implementation|impl_review|finished|chat)_state"`
  → only the compat shims (or nothing once call sites migrate).
- Full pipeline run (explore → plan → review → impl → impl review → finished chat)
  with the fake pi shim completes with the same stage statuses as before the refactor.
