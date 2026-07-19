# paths.py Exploration Report

**Scope**: Investigated `/workspace/.pi/crack/server/src/crack_server/paths.py` — the canonical path-resolution and prompt/state file I/O module for the crack-pi-server.

**Key Files & Symbols**:
- Path constructors: `project_root()`, `tasks_dir()`, `task_dir()`, `info_path()`, `explore_dir()`, `explore_sessions_dir()`, `plan_dir()`, `plan_sessions_dir()`, `implementation_sessions_dir()`, `impl_review_sessions_dir()`, `harness_dir()`, `templates_dir()`, `queue_dir()` family, `unscripted_chats_dir()`, `chat_dir()`, `sub_agents_dir()`, `run_dir()`.
- Validation regexes: `TASK_ID_RE`, `PROMPT_NAME_RE`, `STAGE_SLUG_RE`, `PLAN_ARTEFACT_NAME_RE`, `CHAT_ID_RE`, `PERSONA_SLUG_RE`, `RUN_ID_RE`.
- Prompt file ops: `list_prompt_files()`, `read_prompt()`, `write_prompt()`, `delete_prompt()`, `next_prompt_filename()`, `read_all_prompts_joined()`, `prompts_last_modified()`.
- Task/state accessors: `read_info()`, `write_info()`, `create_task()`, `slugify_title()`, `generate_task_id()`.
- JsonState accessors (per-task): `title_regen_state()`, `explore_state()`, `plan_state()`, `plan_review_state()`, `implementation_state()`, `impl_review_state()`, `finished_state()`.
- Harness-level state: `models_cache_state()`, `stage_config_state()`.
- Plan artefacts: `write_plan_artefact()`, `read_plan_artefact()`, `plan_todo_path()`, `walkthrough_path()`, `read_walkthrough()`.
- Explore artefacts: `write_explore_artefact()`.
- Unscripted chats: `list_chat_ids()`, `generate_chat_id()`, `chat_info_state()`, `chat_state()`, `create_chat()`.
- Sub-agents: `generate_run_id()`, `run_state()`, `run_state_by_id()`, `run_report_path()`, `run_pid_file()`.

**Open Questions**:
- Why does `list_prompt_files()` call `directory.mkdir(parents=True, exist_ok=True)` on a read? (Side-effect on read.)
- `write_prompt()` and `write_stage_template()` don't use atomic writes — is that intentional?
- `JsonState` handles atomic writes; why do these functions bypass it?

**Risks**:
- Non-atomic prompt/template writes could leave partial files on crash.
- Implicit directory creation in readers masks missing-directory bugs.
- No locking on prompt file reads/writes (race conditions possible if worker + web both touch prompts).

**Recommended Next Steps**:
1. Move prompt/template writes to `JsonState`-style atomic writes (or at least write-to-temp+rename).
2. Remove `mkdir` from `list_prompt_files()` / `list_stage_templates()` — require dir to exist.
3. Add a `read_prompt_safe()` that returns `""` on missing file (for idempotent reads).
4. Consider `flock` on prompt writes if concurrent access is expected.