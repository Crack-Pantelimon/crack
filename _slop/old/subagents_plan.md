# Sub-agents for the crack-server harness

## Context

The crack-server harness (`.pi/crack/server`) has a rigid staged pipeline (explore → plan → plan_review → implementation → impl_review → finished) and free-form "unscripted chats" that run a pi agent with all tools. We are adding **sub-agents**: four personas (explorer, planner-debater, coder, tester) the unscripted-chat agent can invoke as pi tool calls, registered by our own pi extension. Each sub-agent runs its own pi session (own context, own model), writes a final report to an auto-generated path, and can recursively spawn sub-agents up to depth 3. The rigid pipeline is untouched and never sees these tools.

## Locked decisions (from user Q&A)

1. **Non-blocking spawn + resume**: spawn tool returns immediately (`run_id` + report path). Child runs as worker queue jobs. On child terminal state, infrastructure enqueues a resume of the parent session with a message = child's last assistant message + report dump (or failure detail).
2. **Transport**: TS extension → crack-server HTTP API on `127.0.0.1:9847`. Server mints run_id, creates run dir, enqueues, returns JSON. Extension is a thin, robust wrapper (fetch timeout, clear thrown errors, no load-time crashes).
3. **Planner Q&A → human** via chat-page forms (reuse `parse_questions`/`collect_answers`/`render_questions_form`); loop grill→answers→write→grill… until human clicks continue, then final report.
4. **Run storage under owning chat**: `.pi/crack/unscripted_chats/<chat_id>/sub_agent_runs/<run_id>/`, flat, tree linked via `parent_run_id`. Chat delete/stop cascades to the whole tree.
5. **MAX_WORKERS = 24** (shared pool; nvidia 40rpm process-global limiter remains the throttle).
6. **One tool per persona**; every persona's own hops get full tools (`tools=None`). Depth via env var; extension registers spawn tools only when depth < 3 and only inside crack chat/sub-agent context.
7. **Failure policy**: run until settle; if report missing and last hop made no tool calls → nudge with exact path + expected content; max 3 nudges then `phase=error`, parent gets failure resume. Runs retryable from UI.
8. **UI**: agent control page (per-persona model select + template editing), chat-page live run-tree fragment (statuses, inline planner forms, stop/retry), per-run trajectory page reusing `render_turn_msgs`.
9. **Chat concurrency**: human messages allowed while children run; human messages and child-completion resumes serialize per chat via the queue (replace B2 refuse-while-chatting with queueing).
10. **Default models**: explorer/planner/coder = `nvidia/nemotron-3-ultra-550b-a55b`, tester = `nvidia/z-ai/glm-5.2`. Coder does **not** auto-commit.
11. **Persona code+templates+config live in `.pi/crack/sub_agents/<persona>/`** (next to `tasks/`, outside the server package): python flow module + prompt `.md` templates + `config.json`. Server loads them via importlib-from-path (mirroring `stages/__init__._discover`).

## Data model

### Directory layout
```
.pi/crack/sub_agents/<persona>/          # persona definition (checked in)
  agent.py                               # exports PERSONA = <PersonaSubclass>()
  config.json                            # {"model": "...", "tool_description": "...", "tool_label": "..."}
  <template>.md                          # prompt templates (system.md, nudge.md; planner adds grill.md/write.md/followup.md)

.pi/crack/unscripted_chats/<chat_id>/sub_agent_runs/<run_id>/   # per-run working dir
  run.json                               # run state (schema below)
  report.md                              # the report the agent writes (the deliverable)
  sessions/                              # pi jsonl session dir (pruned on restart)
  round_N_questions.json / round_N_answers.json   # planner only
  agent.pid                              # running pi pid (for STOP/orphan-kill)
```

`run_id = <ms_epoch>_<uuid8>`. Report path is deterministic: `sub_agent_runs/<run_id>/report.md`.

### `run.json` schema
```
{
  "run_id", "persona", "chat_id",
  "parent_kind": "chat" | "run",        # what to resume on completion
  "parent_id":  <chat_id> | <run_id>,
  "depth": int,                          # 0=chat; children increment; >=3 => no spawn tools
  "instructions": str,                   # text passed by the caller
  "report_path": str,                    # absolute path to report.md
  "phase": "running" | "awaiting_answers" | "resuming" | "writing" | "done" | "error" | "stopped",
  "started_token": uuid,                 # stale-job guard (mirror Stage.prepare_start_token)
  "stop_requested": bool,
  "nudge_count": int,
  "rounds": [ {"questions": [...], "answers": {...}} ],   # planner
  "turns": [ ... ],                      # trajectory (steprun turn shape) -> render_turn_msgs
  "child_inbox": [ {run_id, status, last_message, report_excerpt} ],  # drained by resume step
  "error", "error_detail", "error_step", "finished_at", "created_at"
}
```

## Framework code (`src/crack_server/sub_agents/`)

New package inside the server, mirroring `stages/`:

- **`base.py`** — `SubAgentPersona` base class:
  - class attrs `slug`, `name`, `report_instructions` (what the report must contain), `templates: list[str]`.
  - `model_for()` / `set_model(model_id)` → read/write `sub_agents/<slug>/config.json` (mirror `Stage.model_for`).
  - `load_template(name)` → read `.pi/crack/sub_agents/<slug>/<name>` fresh (mirror `Stage.load_template`).
  - `tool_name()`, `tool_description()`, `tool_label()` from config.json (fed to the extension).
  - `run_step(run_id, step, form) -> (step, form) | None` — persona dispatch, returns successor step (self-chaining like stages).
  - `check_orphaned(run_id)`, `retry(run_id)`, `request_stop(run_id)` — mirror Stage equivalents on run.json.
  - Default generic flow (explorer/coder/tester): step `run` → `_run_hop` (one `run_agent_hop`, `tools=None`, sentinel=None) → verify `report.md` exists via `verify_artifact_file`; if missing and last hop made no tool calls → nudge (successor `("run", {"nudge": True})`), else if missing but tools were called just re-hop; cap at 3 nudges → `phase=error`; on success `phase=done` → `_finish` enqueues parent resume.
- **`planner.py` flow** (persona subclass overrides `run_step`): steps `grill` → (write questions, `awaiting_answers`) → `answers` action resumes with `followup` → loop until human "continue" → `write` (writes plan into report) → `_finish`. Reuse `s02_plan` helpers (`parse_questions`, `collect_answers`, `format_qa_for_prompt`) — extract the shared ones from `s02_plan.py` into `stages/questions.py` (or reuse in place) so no duplication.
- **`registry.py`** — `_discover()` globs `.pi/crack/sub_agents/*/agent.py`, importlib-from-path, requires module `PERSONA`, indexes by slug. `REGISTRY`, `get(slug)`, `list_personas()`.
- **`runner.py`** — `spawn(chat_id, persona, instructions, parent_kind, parent_id, depth) -> run.json dict`: validate persona + `depth < 3`, mint run_id, mkdir run dir, write initial run.json (started_token, phase=running), `queue.enqueue_exclusive(chat_id, SUBAGENT_JOB_SLUG, "run_start", form={run_id, started_token})`. `finish(run_id, status)`: append `{run_id,status,last_message,report_excerpt}` to parent's `child_inbox` (locked RMW), then `enqueue_exclusive(parent_chat, RESUME_JOB_SLUG or chat slug, step="drain_children")` so the parent processes it. One handoff message format built here.
- **`resume.py`** — parent-resume mechanics: a `drain_children` step reads+clears `child_inbox`, formats one message per child result, and runs the parent hop (chat exchange or parent run hop). Serialized per parent via `enqueue_exclusive` on `(parent_id, slug)`; if a child finishes while the parent is mid-hop, the inbox holds it and the next drain job picks it up (the completion `enqueue_exclusive` is a no-op dedupe while one is queued/processing, and `finish` always appends to the inbox first so nothing is lost).

## Worker / queue changes (`worker.py`, `queue.py`)

- `MAX_WORKERS = 24`.
- `_dispatch` routing: new slugs `SUBAGENT_JOB_SLUG = "__subagent__"` (job carries `chat_id`+`run_id`; routes to `sub_agents.registry.get(persona).dispatch_step`) and the parent drain handled through the existing chat slug path (extend `chats.run_chat` to first drain `child_inbox`, or a dedicated `__subagent_resume__` slug). Job `task_id` field carries `chat_id` for exclusivity; add a `run_id` field to the job spec (queue.py job dict) used for run-level exclusivity `(chat_id, run_id)`.
- `_kill_orphaned_agents()`: add glob `unscripted_chats/*/sub_agent_runs/*/agent.pid`.
- `_sweep_orphaned_phases()`: iterate runs and call `persona.check_orphaned(run_id)`.
- `_prune_old_session_dirs()`: include run session dirs.
- `reclaim_orphans` already requeues in-flight jobs → run hops resume via the same transient session-resume path.

## Chat changes (`chats.py`, `chat_engine.py`, `routes_chats.py`)

- Replace B2 refuse-while-chatting: `post_message` always appends an exchange and enqueues; the worker processes exchanges FIFO per chat via `enqueue_exclusive` on `(chat_id, __chat__)` + a pending-exchange queue in state, so a human message and a child-resume serialize instead of colliding.
- Tag exchange source: `{"source": "human" | "child_report", ...}` for rendering.
- Chat pi hop must run with `CRACK_SUBAGENT_CTX` env + `CRACK_SUBAGENT_DEPTH=0` so the extension registers spawn tools. `chat_engine.run_exchange` passes these into `run_agent_hop` (extend `pi_proc._build_cmd` / Popen `env`).
- Long-poll wake: `task_state_mtimes` scan for chats must also stat `sub_agent_runs/*/run.json` (or `finish`/phase transitions `touch` the chat state file) so the run-tree fragment updates live.
- Stop/delete: `stop_chat` and `delete_chat` walk `sub_agent_runs/*`, kill each `agent.pid`, set phases stopped (delete then rm-rf as today).

## HTTP API (`routes_sub_agents.py`, new)

- `POST /api/chats/{chat_id}/sub_agents/spawn` — body `{persona, instructions, parent_kind, parent_id, depth}`; returns `{run_id, report_path, status}`. Depth/persona validated; 4xx with clear JSON body on bad input.
- `GET /api/chats/{chat_id}/sub_agents/runs` + `GET .../runs/{run_id}` — status (for UI and an optional extension status tool).
- `POST .../runs/{run_id}/answers` — planner question form (reuse `collect_answers`), enqueues followup.
- `POST .../runs/{run_id}/continue` — planner "finish debating" → enqueue `write`.
- `POST .../runs/{run_id}/stop`, `POST .../runs/{run_id}/retry`.
- Control page: `GET /sub_agents` (list personas), `POST /api/sub_agents/{slug}/model`, `PUT /api/sub_agents/{slug}/templates/{filename}` — reuse `Stage.render_config_body`-style helpers adapted to personas.
- Run page: `GET /sub_agents/runs/{run_id}` renders `render_turn_msgs(run["turns"])`.

## pi extension (`.pi/extensions/crack_subagents/index.ts`, new)

Separate dir from the existing `crack_pi` (keep its `/crack` commands untouched). Default-export factory:
- On load: read `CRACK_SUBAGENT_CTX` (must be set) and `CRACK_SUBAGENT_DEPTH`. If ctx unset (plain pi / rigid harness) → register nothing. If depth ≥ 3 → register nothing.
- Otherwise fetch persona list from `GET http://127.0.0.1:9847/api/sub_agents` at `session_start`; for each persona `pi.registerTool` with `name` from config, `parameters: Type.Object({ instructions: Type.String() })`, `executionMode: "parallel"`.
- `execute`: POST spawn to the server with `AbortSignal.timeout(...)`; on non-2xx throw `Error(body)`; on network error throw a clear message; on success return `{content:[{type:"text", text: "Spawned <persona> run <run_id>. It will report back to you here; its report will be at <report_path>."}]}`. Output truncated via `truncateTail`. Never throw at module load.
- `CRACK_CHAT_ID` / `CRACK_PARENT_KIND` / `CRACK_PARENT_ID` env (set by the runner) supply the spawn context so the tool knows who its parent is.

## Tests (`tests/`, `python -m pytest`, fake_pi.sh contract)

- Scaffold `.pi/crack/sub_agents/*` into the tmp `CRACK_PI_PROJECT_ROOT` (a fixture copies the checked-in persona dirs, or a seed helper).
- `test_sub_agents.py`: spawn→run→report→parent-resume happy path; nudge path (report missing, no tool calls → nudge → report); nudge exhaustion → error + failure resume; depth-limit gating (no spawn tools at depth 3); parallel children both delivered via `child_inbox`; restart/reclaim resumability (`reclaim_orphans` requeues, session resumes); planner Q&A rounds (questions file → answers → continue → report).
- Extend `fake_pi.sh` if needed to emit a `write`(report) behavior + no-tool-call settle for nudge tests.

## Risks / races addressed

- **Two children finish at once / child finishes mid parent-hop**: `finish` appends to `child_inbox` under flock first, then enqueues an exclusive `drain_children` job; the drain step reads-and-clears the inbox, so all results are delivered even if only one drain job runs.
- **Worker restart mid-hop**: pid files killed, session dirs kept, `reclaim_orphans` requeues; run hop resumes via transient session-resume ("Continue where you left off").
- **Stale start jobs**: `started_token` guard mirrored from `Stage.prepare_start_token`/`dispatch_step`.
- **Depth escalation**: enforced twice — server rejects `depth >= 3` at spawn, and the extension registers no spawn tools when `CRACK_SUBAGENT_DEPTH >= 3`.
- **Rigid harness isolation**: stages pass explicit `--tools` lists and never set `CRACK_SUBAGENT_CTX`, so spawn tools are never visible there.

## Verification

1. `docker exec crack-dev` → `cd .pi/crack/server && python -m pytest tests/test_sub_agents.py -x`.
2. Start server (`uv run crack-server`), open a chat, ask the agent to "explore X with a sub-agent"; confirm the run-tree fragment appears, run reaches `done`, and the parent chat resumes with the report.
3. Trigger the planner via chat; confirm question form renders in-page, answers advance rounds, "continue" produces the plan report.
4. Kill the worker mid-run (`docker exec crack-dev pkill -f crack-server.*worker` equiv) and confirm the run resumes after restart.
5. Verify depth: have a sub-agent spawn a sub-agent (depth 1→2→3) and confirm the depth-3 pi session sees no spawn tools.
6. Load the extension standalone (`pi -e .pi/extensions/crack_subagents/index.ts --print --no-session ping` with no ctx env) → registers nothing, no error.
