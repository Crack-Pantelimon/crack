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
.pi/crack/server/tests/test_pi_rpc.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_plan41.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_sandbox.py ← __future__, unittest, crack_server, pytest
.pi/crack/server/tests/test_stop_durable.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_sub_agents.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_trajectory_view.py ← __future__, crack_server
```

## changes (last 5 commits — 6 minutes ago)
```
.pi/crack/server/tests/test_pi_rpc.py         +_hop_kwargs  +_fake_launch  +test_rpc_persists_turn_and_returns_agent_end  +test_rpc_stop_check_sends_abort_and_returns_stopped
.pi/crack/server/tests/test_plan41.py         +_fake_rpc_launch  +test_hard_failure_after_persisted_turns_raises_exact_error  +test_terminal_linger_past_grace_is_not_sigkill  +test_auto_retry_end_after_progress_raises_exact_error
.pi/crack/server/tests/test_sandbox.py        +test_exec_in_interactive_adds_i_flag  +test_exec_in_passes_stream_limit  +test_exec_in_omits_limit_when_unset  ~test_ensure_sandbox_creates_with_overlay_dirs
.pi/crack/server/tests/test_stop_durable.py   +noop_enqueue  +test_stop_chat_sets_stop_requested  +test_pop_pending_drains_queue_while_stopped  +_seed
.pi/crack/server/tests/test_sub_agents.py     ~fake_pi
.pi/crack/server/tests/test_trajectory_view.py +test_merge_exchange_sidecars_interleaves_errors_by_time  ~test_ansi_to_html_preserves_colour
```

## .pi

### .pi/crack/server/tests/test_pi_rpc.py
```
async def test_rpc_persists_turn_and_returns_agent_end(tmp_path, monkeypatch)  :47-56
async def test_rpc_stop_check_sends_abort_and_returns_stopped(tmp_path, monkeypatch)  :60-87
```

### .pi/crack/server/tests/test_plan41.py
```
class FakePi  :24-40
  def __init__(ctrl: Path, script: Path)
  def set_script(lines: list[str]) → None
  def argv(n: int) → list[str]
  def prompt(n: int) → str
  def invocations() → int
def fake_pi  :56-75
def run_hop  :78-94
def test_limiter_keyed_by_provider  :102-107
def test_rate_limiter_reserves_slots_without_serializing  :110-130
def test_non_nvidia_hops_run_back_to_back  :133-139
def test_is_transient_classification  :147-155
def test_transient_then_success_completes_one_trajectory  :158-166
def test_midstream_kill_resumes_session_with_continuation  :169-179
def test_transient_failures_raise_at_streak_cap  :182-190
def test_hard_failure_after_persisted_turns_raises_exact_error  :193-207
def test_error_budget_cap_raises_over_budget  :210-221
def test_broken_error_recorder_never_wedges_retries  :224-232
def test_terminal_linger_past_grace_is_not_sigkill  :240-251
def test_nonzero_exit_without_terminal_still_retries  :254-266
def test_auto_retry_end_after_progress_raises_exact_error  :269-284
def test_empty_turns_returns_empty_reason  :287-297
def test_persisted_then_clean_agent_end_still_returns_immediately  :300-312
def test_willretry_agent_end_does_not_end_hop_early  :315-337
def test_rpc_message_update_error_surfaces_exact_text  :340-346
def test_hard_backoff_schedule_matches_hard_retry_delays  :349-369
def test_run_pi_text_transient_then_ok  :372-376
def test_run_pi_text_hard_failures_exhaust_schedule  :379-383
def test_run_pi_text_records_each_failed_attempt  :386-402
def test_forty_turns_stream_uncut  :410-414
```

### .pi/crack/server/tests/test_sandbox.py
```
def host_env(monkeypatch, tmp_path)  :14-16
async def test_sandbox_enabled_off_in_tests(host_env, monkeypatch, tmp_path)  :20-23
async def test_sandbox_enabled_forced(monkeypatch, host_env)  :27-30
async def test_ensure_network_creates_when_missing(host_env)  :34-47
async def test_ensure_network_skips_create_when_present(host_env)  :51-61
async def test_ensure_sandbox_starts_existing(host_env)  :65-79
async def test_ensure_sandbox_creates_with_overlay_dirs(monkeypatch, tmp_path)  :83-129
async def test_exec_in_interactive_adds_i_flag(host_env)  :133-147
async def test_exec_in_passes_stream_limit(host_env)  :151-166
async def test_exec_in_omits_limit_when_unset(host_env)  :170-174
async def test_exec_in_builds_command(host_env)  :178-195
async def test_kill_session_escalates_to_kill(host_env)  :199-219
async def test_destroy_sandbox_kill_and_rm(host_env)  :223-235
async def test_destroy_sandbox_noop_when_missing(host_env)  :239-251
```

### .pi/crack/server/tests/test_stop_durable.py
```
def noop_enqueue(monkeypatch)  :13-14
def test_stop_chat_sets_stop_requested(chat_root, monkeypatch)  :17-20
def test_pop_pending_drains_queue_while_stopped(chat_root)  :23-35
def test_enqueue_system_message_preserves_stop(chat_root, noop_enqueue)  :38-44
def test_merge_child_inbox_preserves_stop(chat_root, noop_enqueue)  :47-66
def test_post_message_clears_stop(chat_root, noop_enqueue)  :69-73
def test_answer_chat_question_clears_stop(chat_root, noop_enqueue)  :76-84
async def test_exchange_finish_preserves_stop_requested(chat_root)  :88-114
def test_subagent_stop_does_not_clear_parent_stop(chat_root, fake_pi, monkeypatch)  :117-135
def test_subagent_retry_clears_only_run_stop(chat_root, fake_pi, monkeypatch)  :138-163
```

### .pi/crack/server/tests/test_sub_agents.py
```
def fake_pi(tmp_path, monkeypatch) → FakePi  :42-62
def chat_root(tmp_path, monkeypatch, fake_pi) → str  :66-72
def test_personas_discovered(chat_root)  :75-77
def test_active_child_count_endpoint(chat_root)  :80-97
async def test_spawn_run_report_parent_resume(chat_root, fake_pi)  :102-127
async def test_nudge_then_report(chat_root, fake_pi)  :132-148
async def test_nudge_exhaustion_errors_and_resumes_parent(chat_root, fake_pi)  :153-169
async def test_depth_limit_rejects_spawn_beyond_max(chat_root, fake_pi)  :174-203
async def test_parallel_children_both_delivered(chat_root, fake_pi)  :208-234
async def test_reclaim_orphans_requeues(chat_root, fake_pi)  :239-259
def test_api_list_personas(chat_root)  :263-268
async def test_api_spawn(chat_root, fake_pi)  :273-304
def test_sidebar_tree_always_polls(chat_root)  :307-309
def test_sidebar_tree_shows_spawned_run(chat_root)  :312-323
async def test_run_gets_title_on_begin(chat_root, fake_pi)  :327-339
def test_sidebar_node_shows_title_not_persona(chat_root)  :342-359
def test_sidebar_order_and_metrics(chat_root, monkeypatch)  :362-392
def test_fill_template_depth_gating(chat_root)  :395-406
async def test_spawn_parallel_cap_slot_pending(chat_root, monkeypatch)  :410-448
async def test_spawn_waits_for_free_slot(chat_root, fake_pi)  :452-493
```

### .pi/crack/server/tests/test_trajectory_view.py
```
def test_project_unknown_event_has_expand_row(tmp_path: Path)  :11-43
def test_project_merges_tool_results(tmp_path: Path)  :46-78
def test_ansi_to_html_preserves_colour()  :81-86
def test_merge_exchange_sidecars_interleaves_errors_by_time()  :89-137  # Errors with ``at`` between turn timestamps appear in order, 
def test_host_worktree_dirty_detects_untracked(tmp_path: Path)  :140-150
```

## _data

### _data/news/main.py
```
def main()  :9-26
```

## _docker

### _docker/_apply_healthcheck.sh
```
# Plan 7 Part B (steps 3-4): after a self-modifying patch is applied to the live
function log()
```

### _docker/_blender_mcp_lazy.sh
```
# Lazy Blender bootstrap for stdio blender-mcp (sandboxes and crack-dev pi agents).
function port_open()
function start_blender_stack()
```

### _docker/_cont_start.sh
```
function respawn()
export CRACK_PI_HOST
export MCP_FIREFOX_PORT
export MCP_CHROMIUM_PORT
export MCP_WEBSEARCH_PORT
export BROWSER_HEADLESS
export MCP_BLENDER_HTTP_PORT
export QT_QPA_PLATFORM
export WAYLAND_DISPLAY
export DISPLAY
```

### _docker/_sandbox_common.sh
```
# Shared cheap setup for crack-dev and sandboxes: env exports, MCP config, Blender addon sync.
export CRACK_PI_PROJECT_ROOT
export CRACK_HARNESS_DATA_DIR
export HOME
export PATH
export CHROME_BIN
export CHROME_PATH
export CHROMIUM_PATH
export FIREFOX_BIN
export CHROMEDRIVER_BIN
export GECKODRIVER_BIN
export PLAYWRIGHT_BROWSERS_PATH
export CHROMIUM_FLAGS
export BLENDER_ADDON_PORT
export BLENDER_HOST
export BLENDER_PORT
export DISABLE_TELEMETRY
```

### _docker/_sandbox_start.sh
```
# Cheap sandbox entrypoint: shared env + lazy MCP config, no eager HTTP bridges or crack-server.
```

### _docker/run.sh
```
export IMG_NAME
```
