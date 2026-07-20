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
.pi/crack/server/src/crack_server/titles.py ← __future__, crack_server
.pi/crack/server/src/crack_server/transcript.py ← __future__, crack_server
.pi/crack/server/src/crack_server/ui.py ← __future__, markdown_it, crack_server
.pi/crack/server/src/crack_server/worker.py ← __future__
.pi/crack/server/tests/test_b13_stderr_tail.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_plan_revamp.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_plan41.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_stage_lifecycle.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_state.py ← __future__, crack_server
.pi/crack/server/tests/test_sub_agents.py ← __future__, crack_server, tests, pytest
```

## todos
```
.pi/crack/server/tests/test_plan_revamp.py:143  # TODO: was regenerated (single-shot) and Plan Review auto-started.
```

## changes (last 5 commits — 12 hours ago)
```
.pi/crack/server/src/crack_server/worker.py   +_reopen  +_sweep_orphaned_phases  ~_dispatch  ~_kill_orphaned_agents
.pi/crack/server/tests/test_plan_revamp.py    +fake_pi  +task  +plan_stage  +review_stage
.pi/crack/server/tests/test_plan41.py         ~test_enqueue_exclusive_drops_duplicates
.pi/crack/server/tests/test_sub_agents.py     +_seed_personas  +_drain_jobs  +fake_pi  +chat_root
```

## .pi

### .pi/crack/server/src/crack_server/titles.py
```
def generate_title  :21-35
async def agenerate_title  :38-47
```

### .pi/crack/server/src/crack_server/transcript.py
```
def text_from_content  :24-34
def apply_event_to_turn  :37-104
def turn_has_content  :107-112
def count_turn_groups  :115-129
def truncate_output  :132-146
def tail_truncate  :149-153
def fit_nano_transcript  :156-161
def render_transcript_plaintext  :164-186
def resolve_path_ref  :189-213
def extract_path_refs  :216-243
def read_file_lines  :246-270
```

### .pi/crack/server/src/crack_server/ui.py
```
def render_file_row(view_url: str, save_url: str, name: str, content: str, meta: str, editing: bool, *, extra_actions: str, indent: str) → str  :143-152
```

### .pi/crack/server/src/crack_server/worker.py
```
async def async_loop() → None  :286-326  # Claim and dispatch jobs forever, one asyncio task per job (n
def start_background() → asyncio.Task  :329-331  # Lifespan hook: start the worker loop as a background task
async def stop_background(task: asyncio.Task) → None  :334-338  # Lifespan hook: cancel the worker loop and let it reap in-fli
def main() → None  :341-346  # Deprecated: the worker now runs inside the server process (a
```

### .pi/crack/server/tests/fake_pi.sh
```
# Fake `pi` for tests — copied onto PATH as `pi`, ahead of the real binary.
function emit_turn()
```

### .pi/crack/server/tests/test_b13_stderr_tail.py
```
def fake_pi_script(tmp_path, monkeypatch) → Path  :25-42
def run_hop(tmp_path, message)  :45-57
def test_compose_detail_prefers_stderr_and_labels()  :60-66
def test_hop_hard_failure_detail_contains_stderr(fake_pi_script, tmp_path)  :69-75
def test_hop_midfail_detail_prefers_stderr_over_json_events(fake_pi_script, tmp_path)  :78-86
def test_run_pi_text_hard_failure_detail_contains_stderr(fake_pi_script)  :89-95
```

### .pi/crack/server/tests/test_plan_revamp.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :62-79
def task(tmp_path, monkeypatch, fake_pi) → str  :83-88
def plan_stage()  :91-96
def review_stage()  :99-104
async def test_draft_chains_to_write_through_worker_cycle(task, fake_pi, tmp_path)  :114-147
async def test_write_step_corrective_retry_names_deficiency(task, fake_pi, tmp_path)  :151-171
async def test_critique_no_questions_chains_to_verified_revise(task, fake_pi)  :175-196
def test_run_until_verified_passes_first_try()  :217-225
def test_run_until_verified_corrective_then_pass()  :228-237
def test_run_until_verified_exhausts_correctives()  :240-249
def test_run_until_verified_time_cap_still_verifies()  :252-255
def test_run_until_verified_stopped_runs_callback()  :258-263
def test_verify_artifact_file_checks(tmp_path)  :271-287
def test_verify_artifact_file_heading_prefix_match(tmp_path)  :290-296
def test_check_orphaned_flags_running_phase_without_job(task, fake_pi)  :309-322
def test_check_orphaned_leaves_backed_and_settled_phases_alone(task, fake_pi)  :325-342
```

### .pi/crack/server/tests/test_plan41.py
```
class FakePi  :25-41
  def __init__(ctrl: Path, script: Path)
  def set_script(lines: list[str]) → None
  def argv(n: int) → list[str]
  def prompt(n: int) → str
  def invocations() → int
def fake_pi(tmp_path, monkeypatch) → FakePi  :45-62
def run_hop(tmp_path, message, sentinel, model, **kw)  :65-80
def test_limiter_keyed_by_provider()  :88-93
def test_rate_limiter_reserves_slots_without_serializing()  :96-116
def test_non_nvidia_hops_run_back_to_back(fake_pi, tmp_path)  :119-125
def test_is_transient_classification()  :133-141
def test_transient_then_success_completes_one_trajectory(fake_pi, tmp_path)  :144-151
def test_midstream_kill_resumes_session_with_continuation(fake_pi, tmp_path)  :154-170
def test_four_transient_failures_raise(fake_pi, tmp_path)  :173-177
def test_hard_failure_after_persisted_turns_raises_immediately(fake_pi, tmp_path)  :180-198
def test_run_pi_text_transient_then_ok(fake_pi)  :201-205
def test_run_pi_text_hard_failures_exhaust_schedule(fake_pi)  :208-212
def test_forty_turns_stream_uncut(fake_pi, tmp_path)  :220-224
def test_sentinel_own_line_only(fake_pi, tmp_path)  :232-237
def test_stop_kills_process_group_and_returns_stopped(fake_pi, tmp_path)  :245-276
def test_enqueue_exclusive_drops_duplicates(tmp_path, monkeypatch)  :284-313
def test_prompt_entries_skipped_by_turn_helpers()  :321-331
```

### .pi/crack/server/tests/test_stage_lifecycle.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :28-46
def task(tmp_path, monkeypatch, fake_pi) → str  :50-55
def explore_stage()  :58-63
def test_explore_run_records_compiled_prompts(task, fake_pi)  :66-87
def test_explore_retry_resumes_session(task, fake_pi)  :90-117
def test_message_action_clears_error_and_enqueues_resume(task, fake_pi)  :120-143
def test_stale_start_job_dropped_by_token(task, fake_pi)  :146-160
def test_double_start_enqueues_exactly_one_job(task, fake_pi)  :163-173
```

### .pi/crack/server/tests/test_state.py
```
def test_read_tolerant_of_missing_and_corrupt(tmp_path)  :38-44
def test_write_and_read_roundtrip(tmp_path)  :47-52
def test_write_skips_when_parent_dir_is_gone(tmp_path, caplog)  :55-68  # B7: a straggler write must not resurrect a deleted task/chat
def test_update_loses_no_fields_under_threads(tmp_path)  :71-86
def test_update_loses_no_fields_across_processes(tmp_path)  :89-106
def test_update_loses_no_fields_threads_and_processes_mixed(tmp_path)  :109-136  # The real deployment shape: web-process threads + a separate 
```

### .pi/crack/server/tests/test_sub_agents.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :41-58
def chat_root(tmp_path, monkeypatch, fake_pi) → str  :62-68
def test_personas_discovered(chat_root)  :71-73
async def test_spawn_run_report_parent_resume(chat_root, fake_pi)  :78-103
async def test_nudge_then_report(chat_root, fake_pi)  :108-124
async def test_nudge_exhaustion_errors_and_resumes_parent(chat_root, fake_pi)  :129-145
async def test_depth_limit_rejects_spawn_beyond_max(chat_root, fake_pi)  :150-179
async def test_parallel_children_both_delivered(chat_root, fake_pi)  :184-210
async def test_reclaim_orphans_requeues(chat_root, fake_pi)  :215-235
async def test_planner_qa_round_then_write(chat_root, fake_pi)  :240-294
def test_api_list_personas(chat_root)  :297-302
async def test_api_spawn(chat_root, fake_pi)  :307-338
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
