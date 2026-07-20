The game is in folder `crack_demo/demo_resolution_selector_web_bevy`.

Base rust packages are under `rust_pkg`. 

Data/asset generation and pre-procesing is in `_data`.

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## deps
```
.pi/crack/server/src/crack_server/sub_agents/wait.py ← __future__, crack_server
.pi/crack/server/src/crack_server/titles.py ← __future__, crack_server
.pi/crack/server/src/crack_server/worker.py ← __future__
.pi/crack/server/tests/test_ask_user.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_async_worker.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_plan_revamp.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_plan41.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_sub_agents.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_wait_join.py ← __future__, crack_server, tests, pytest
```

## todos
```
.pi/crack/server/tests/test_plan_revamp.py:144  # TODO: was regenerated (single-shot) and Plan Review auto-started.
```

## .pi

### .pi/crack/server/src/crack_server/sub_agents/wait.py
```
def drain_matching  :82-86
def poll  :135-142
def stamp_waiting  :220-221
def clear_waiting  :235-241
```

### .pi/crack/server/src/crack_server/titles.py
```
def generate_title(content: str, *, log_prefix: str) → str  :21-35  # Summarize ``content`` into a short title via the nano title 
async def agenerate_title(content: str, *, log_prefix: str) → str  :38-47  # Async twin of :func:`generate_title` (chat titles, in the ev
```

### .pi/crack/server/src/crack_server/worker.py
```
def recover_detached_hops() → None  :148-207
async def async_loop() → None  :334-374  # Claim and dispatch jobs forever, one asyncio task per job (n
def start_background() → asyncio.Task  :377-379  # Lifespan hook: start the worker loop as a background task
async def stop_background(task: asyncio.Task) → None  :382-386  # Lifespan hook: cancel the worker loop and let it reap in-fli
def main() → None  :389-394  # Deprecated: the worker now runs inside the server process (a
```

### .pi/crack/server/tests/fake_pi.sh
```
# Fake `pi` for tests — copied onto PATH as `pi`, ahead of the real binary.
function emit_turn()
```

### .pi/crack/server/tests/test_ask_user.py
```
async def test_ask_user_suspends_run_then_answer_resumes(chat_root, fake_pi)  :22-62
async def test_ask_user_orphan_sweep_skips_awaiting_user(chat_root, fake_pi)  :66-87
async def test_ask_user_answer_requires_awaiting_phase(chat_root, fake_pi)  :91-104
async def test_ask_user_route_and_chat_parent(chat_root, fake_pi)  :108-142
async def test_user_answer_route(chat_root, fake_pi)  :146-176
```

### .pi/crack/server/tests/test_async_worker.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :19-37
async def test_two_chat_hops_interleave(tmp_path, monkeypatch, fake_pi)  :62-87  # Two 2s-sleeping chat hops dispatched concurrently finish in 
async def test_enqueue_fires_wakeup_callback(tmp_path, monkeypatch, fake_pi)  :91-99
```

### .pi/crack/server/tests/test_plan_revamp.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :62-80
def task(tmp_path, monkeypatch, fake_pi) → str  :84-89
def plan_stage()  :92-97
def review_stage()  :100-105
async def test_draft_chains_to_write_through_worker_cycle(task, fake_pi, tmp_path)  :115-148
async def test_write_step_corrective_retry_names_deficiency(task, fake_pi, tmp_path)  :152-172
async def test_critique_no_questions_chains_to_verified_revise(task, fake_pi)  :176-197
def test_run_until_verified_passes_first_try()  :218-226
def test_run_until_verified_corrective_then_pass()  :229-238
def test_run_until_verified_exhausts_correctives()  :241-250
def test_run_until_verified_time_cap_still_verifies()  :253-256
def test_run_until_verified_stopped_runs_callback()  :259-264
def test_verify_artifact_file_checks(tmp_path)  :272-288
def test_verify_artifact_file_heading_prefix_match(tmp_path)  :291-297
def test_check_orphaned_flags_running_phase_without_job(task, fake_pi)  :310-323
def test_check_orphaned_leaves_backed_and_settled_phases_alone(task, fake_pi)  :326-343
```

### .pi/crack/server/tests/test_plan41.py
```
class FakePi  :31-47
  def __init__(ctrl: Path, script: Path)
  def set_script(lines: list[str]) → None
  def argv(n: int) → list[str]
  def prompt(n: int) → str
  def invocations() → int
def fake_pi(tmp_path, monkeypatch) → FakePi  :51-69
def run_hop(tmp_path, message, sentinel, model, **kw)  :72-87
def test_limiter_keyed_by_provider()  :95-100
def test_rate_limiter_reserves_slots_without_serializing()  :103-123
def test_non_nvidia_hops_run_back_to_back(fake_pi, tmp_path)  :126-132
def test_is_transient_classification()  :140-148
def test_transient_then_success_completes_one_trajectory(fake_pi, tmp_path)  :151-158
def test_midstream_kill_resumes_session_with_continuation(fake_pi, tmp_path)  :161-177
def test_transient_failures_raise_at_streak_cap(fake_pi, tmp_path)  :180-189
def test_hard_failure_after_persisted_turns_resumes_and_retries(fake_pi, tmp_path)  :192-222
def test_error_budget_cap_raises_over_budget(fake_pi, tmp_path)  :225-236
def test_broken_error_recorder_never_wedges_retries(fake_pi, tmp_path)  :239-248
def test_hard_backoff_schedule_is_1_3_9_27(monkeypatch)  :251-262
def test_no_progress_streak_resets_on_progress(fake_pi, tmp_path, monkeypatch)  :265-297
def test_run_pi_text_transient_then_ok(fake_pi)  :300-304
def test_run_pi_text_hard_failures_exhaust_schedule(fake_pi)  :307-311
def test_run_pi_text_records_each_failed_attempt(fake_pi)  :314-328
def test_forty_turns_stream_uncut(fake_pi, tmp_path)  :336-340
def test_sentinel_own_line_only(fake_pi, tmp_path)  :348-353
def test_stop_kills_process_group_and_returns_stopped(fake_pi, tmp_path)  :361-392
def test_enqueue_exclusive_drops_duplicates(tmp_path, monkeypatch)  :400-429
def test_prompt_entries_skipped_by_turn_helpers()  :437-447
```

### .pi/crack/server/tests/test_sub_agents.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :41-59
def chat_root(tmp_path, monkeypatch, fake_pi) → str  :63-69
def test_personas_discovered(chat_root)  :72-74
async def test_spawn_run_report_parent_resume(chat_root, fake_pi)  :79-104
async def test_nudge_then_report(chat_root, fake_pi)  :109-125
async def test_nudge_exhaustion_errors_and_resumes_parent(chat_root, fake_pi)  :130-146
async def test_depth_limit_rejects_spawn_beyond_max(chat_root, fake_pi)  :151-180
async def test_parallel_children_both_delivered(chat_root, fake_pi)  :185-211
async def test_reclaim_orphans_requeues(chat_root, fake_pi)  :216-236
async def test_planner_qa_round_then_write(chat_root, fake_pi)  :241-295
def test_api_list_personas(chat_root)  :298-303
async def test_api_spawn(chat_root, fake_pi)  :308-339
```

### .pi/crack/server/tests/test_wait_join.py
```
async def test_wait_drains_chat_inbox_then_drain_job_noops(chat_root, fake_pi)  :60-88
async def test_wait_run_parent_drain_no_duplicate_child_results(chat_root, fake_pi)  :92-129
async def test_wait_target_resolution(chat_root, fake_pi)  :133-189
async def test_notified_gap_not_misread_as_delivered(chat_root, fake_pi)  :193-213  # notified=true with no inbox entry (the finish() two-write ga
async def test_wait_route_long_poll_wakes_on_notify(chat_root, fake_pi)  :217-245
async def test_wait_route_validation(chat_root, fake_pi)  :249-267
async def test_orphan_check_skips_waiting_parent(chat_root, fake_pi)  :271-291
```

## _docker

### _docker/_cont_start.sh
```
function respawn()
export CRACK_PI_PROJECT_ROOT
export HOME
export CRACK_PI_HOST
export PATH
export CHROME_BIN
export CHROME_PATH
export CHROMIUM_PATH
export FIREFOX_BIN
export CHROMEDRIVER_BIN
export GECKODRIVER_BIN
export PLAYWRIGHT_BROWSERS_PATH
export CHROMIUM_FLAGS
export MCP_FIREFOX_PORT
export MCP_CHROMIUM_PORT
export MCP_WEBSEARCH_PORT
export BROWSER_HEADLESS
```
