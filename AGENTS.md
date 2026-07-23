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
.pi/crack/server/tests/test_model_switch.py ← __future__, crack_server, tests
.pi/crack/server/tests/test_patch.py ← __future__, unittest, crack_server, pytest
.pi/crack/server/tests/test_sandbox.py ← __future__, unittest, crack_server, pytest
.pi/crack/server/tests/test_stop_durable.py ← __future__, crack_server, tests, pytest
.pi/crack/server/tests/test_trajectory_view.py ← __future__, crack_server, pytest
.pi/crack/server/tests/test_vision_media.py ← __future__, fastapi, starlette, crack_server, tests
```

## changes (last 5 commits — 2 hours ago)
```
.pi/crack/server/tests/test_model_switch.py   +test_terminal_reason_row_labels  +test_prep_timing_row_shows_elapsed  ~test_reason_note_shown_for_notable_reasons
.pi/crack/server/tests/test_stop_durable.py   +test_stop_chat_stamps_terminal_reason_and_clears_phase  +_seed  ~test_stop_chat_sets_stop_requested
.pi/crack/server/tests/test_trajectory_view.py +test_merge_exchange_sidecars_appends_terminal_reason  ~test_merge_exchange_sidecars_interleaves_errors_by_time
.pi/crack/server/tests/test_vision_media.py   +fake_analyze  ~fake_analyze  ~_write_png
```

## .pi

### .pi/crack/server/tests/test_model_switch.py
```
def test_make_turn_records_model_when_set  :27-29
def test_make_turn_omits_model_when_empty  :32-34
def test_persister_stamps_current_model  :37-46
def test_persister_stamp_reason_on_last_turn  :49-58
def test_persister_stamp_reason_noop_when_empty  :61-66
def test_reason_note_shown_for_notable_reasons  :69-74
def test_terminal_reason_row_labels  :77-84
def test_prep_timing_row_shows_elapsed  :87-93
def test_model_tag_shown_per_turn  :105-110
def test_prewalk_swap_divider_after_todo  :113-121
def test_user_switch_divider_without_todo  :124-128
def test_no_divider_when_model_stable  :131-134
def test_model_state_threads_across_calls  :137-144
def test_tool_output_short_has_no_expand_toggle  :152-155
def test_tool_output_long_has_single_icon_toggle  :158-163
def test_plan_chat_form_editor_before_first_message  :175-182
def test_plan_chat_form_locked_before_graduation  :185-191
def test_plan_chat_form_dropdown_after_graduation  :194-204
def test_nonplan_chat_form_has_dropdown  :207-212
def test_run_display_model_uses_planner_while_planning  :220-225
def test_run_display_model_uses_implementer_after_swap  :228-237
def test_chat_display_model_planning_then_graduated  :240-248
def test_graduation_gate_matches_prewalk_swap  :251-259
def test_post_message_locks_config_on_first_message  :262-275
def test_dirty_git_gate_preserves_plan_config  :278-294
def test_config_editor_emits_config_hidden_field  :297-304
def test_nonplan_model_resolution_ignores_implementer_until_graduated  :307-324
def test_chat_display_model_prefers_cached  :327-331
def test_image_models_filters_to_image_capable  :339-347
def test_image_models_fallback_when_no_info  :350-356
```

### .pi/crack/server/tests/test_patch.py
```
def artifact_dir(tmp_path)  :14-17
async def test_capture_baseline_writes_tree(artifact_dir)  :21-36
async def test_extract_patch_empty_diff(artifact_dir)  :40-61
async def test_produce_diff_seeds_index_from_base_tree(artifact_dir)  :65-96  # Tracked-but-gitignored files must not spuriously appear as d
async def test_extract_patch_nag_on_big_file(artifact_dir)  :100-115
def test_format_big_file_nag_lists_paths()  :118-121
def test_format_apply_failure_includes_patch_path(tmp_path)  :124-129
def test_record_chat_apply_failure_sets_error_without_enqueue(tmp_path, monkeypatch)  :132-157
async def test_finalize_chat_sandbox_apply_failure_does_not_enqueue(tmp_path, monkeypatch)  :161-193
def test_notify_parent_apply_failure_chat_records_error_not_enqueues(tmp_path, monkeypatch)  :196-217
def test_patch_touches_self_mod_server(tmp_path)  :223-229
def test_patch_touches_self_mod_extension(tmp_path)  :232-235
def test_patch_touches_self_mod_ignores_other_paths(tmp_path)  :238-241
def test_format_test_failure_mentions_untouched_host(tmp_path)  :244-249
def drain_chat(tmp_path, monkeypatch)  :271-277
def test_drain_applies_in_dispatch_order(drain_chat, monkeypatch)  :280-307
def test_drain_defers_while_siblings_running(drain_chat, monkeypatch)  :310-328
def test_drain_conflict_notifies_and_clears(drain_chat, monkeypatch)  :331-351
def test_drain_apply_exception_leaves_pending(drain_chat, monkeypatch)  :354-374
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
def test_stop_chat_stamps_terminal_reason_and_clears_phase(chat_root, monkeypatch)  :23-38
def test_pop_pending_drains_queue_while_stopped(chat_root)  :41-53
def test_enqueue_system_message_preserves_stop(chat_root, noop_enqueue)  :56-62
def test_merge_child_inbox_preserves_stop(chat_root, noop_enqueue)  :65-84
def test_post_message_clears_stop(chat_root, noop_enqueue)  :87-91
def test_answer_chat_question_clears_stop(chat_root, noop_enqueue)  :94-102
async def test_exchange_finish_preserves_stop_requested(chat_root)  :106-132
def test_subagent_stop_does_not_clear_parent_stop(chat_root, fake_pi, monkeypatch)  :135-153
def test_subagent_retry_clears_only_run_stop(chat_root, fake_pi, monkeypatch)  :156-181
```

### .pi/crack/server/tests/test_trajectory_view.py
```
def test_project_unknown_event_has_expand_row(tmp_path: Path)  :13-45
def test_project_merges_tool_results(tmp_path: Path)  :48-80
def test_ansi_to_html_preserves_colour()  :83-88
def test_merge_exchange_sidecars_interleaves_errors_by_time()  :91-139  # Errors with ``at`` between turn timestamps appear in order, 
def test_merge_exchange_sidecars_appends_terminal_reason()  :142-168
def test_merge_exchange_sidecars_duration_falls_back_to_turn_span()  :171-183
def test_host_worktree_dirty_detects_untracked(tmp_path: Path)  :186-196
```

### .pi/crack/server/tests/test_vision_media.py
```
def root(tmp_path, monkeypatch)  :36-38
def test_run_pi_text_image_args(fake_pi)  :70-82
def test_run_pi_text_no_image_args_unchanged(fake_pi)  :85-89
async def test_vision_analyze_rejects_missing_and_invalid(root)  :98-118
async def test_vision_analyze_happy_path(root, monkeypatch)  :122-130
async def test_vision_analyze_resolves_relative_paths(root, monkeypatch)  :134-142
def test_chat_media_route(root)  :151-157
def test_run_media_route(root)  :160-170
def test_persister_attaches_media_only_for_valid_images(root)  :178-208
def test_persister_without_media_dir_leaves_blocks_alone(root)  :211-216
def test_add_attachment_validates_and_describes(root, monkeypatch)  :224-240
async def test_attachment_upload_route(root, monkeypatch)  :244-273
def test_format_block_shape()  :276-290
def test_chat_post_message_weaves_then_clears(root)  :294-310
def test_chat_post_message_stashes_media_onto_the_exchange(root)  :318-334
def test_render_exchanges_shows_prompt_thumbs_from_exchange_media()  :337-344
def test_render_user_prompt_msg_renders_media_thumbs()  :347-361
def test_prompt_recorder_attaches_media_list_and_callable(tmp_path)  :364-382
```

## _docker

### _docker/run.sh
```
export IMG_NAME
```
