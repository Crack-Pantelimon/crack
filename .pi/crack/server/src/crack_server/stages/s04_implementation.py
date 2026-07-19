"""Stage s04: Implementation — a real tool-using agent that implements the
approved plan, keeping a walkthrough and todo, committing when the stage completes.

Modeled on Plan Review's hop loop. Runs kimi-k2.6 by default and switches to a
glm fallback after >10 turns or two consecutive turns that fail the same tool the
same way. Every 5 turns it is reminded to update the todo file. On completion
(``IMPLEMENTATION_COMPLETE`` sentinel or caps) it auto-starts Implementation
Review. All ``pi`` work runs in the worker via ``run_step``.
"""

from __future__ import annotations

import logging
import re
import shutil
import time

from crack_server import git_utils, paths, pi_runner
from crack_server.stages.base import (
    Part,
    Stage,
    render_error_msg,
    render_message_form,
    render_retry_button,
    render_spinner,
    render_stop_button,
    render_turns_trajectory,
)
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

KIMI_MODEL = "nvidia/moonshotai/kimi-k2.6"
GLM_MODEL = "nvidia/z-ai/glm-5.2"

IMPL_SENTINEL = "IMPLEMENTATION_COMPLETE"
IMPL_TIMEOUT_SECONDS = 3600
IMPL_TODO_REMINDER_EVERY = 5

RUNNING_PHASES = ("running",)

# Hard failure markers only (B10): the fallback switch should fire on genuinely
# broken tool runs, not on any output that merely mentions "error"/"failed".
_ERROR_MARKERS = (
    "traceback",
    "command not found",
    "no such file or directory",
    "fatal:",
    "exit code",
)


def _esc(text: str) -> str:
    return _ui._esc(text)


def _looks_failed(output: str) -> bool:
    low = output.lower()
    return any(marker in low for marker in _ERROR_MARKERS)


def _normalize_output(output: str) -> str:
    """Collapse whitespace and clip so equal failures compare equal across turns."""
    return re.sub(r"\s+", " ", output.strip().lower())[:200]


def _turn_error_signature(turn: dict) -> tuple[str, str] | None:
    """(tool_name, normalized_error_output) for the first failing tool block, else None."""
    for block in turn.get("tool_blocks", []):
        output = str(block.get("output", ""))
        if output and _looks_failed(output):
            return (str(block.get("name", "tool")), _normalize_output(output))
    return None


def _has_consecutive_error(turns: list[dict]) -> bool:
    """True when two adjacent turns share the same failing-tool signature."""
    prev: tuple[str, str] | None = None
    for turn in turns:
        sig = _turn_error_signature(turn)
        if sig is not None and sig == prev:
            return True
        prev = sig
    return False


class S04Implementation(Stage):
    slug = "implementation"
    name = "Implementation"
    parts = [
        Part("primary", "Primary agent", "handoff.md", KIMI_MODEL),
        Part("fallback", "Fallback agent", "handoff.md", GLM_MODEL),
    ]

    phase_key = "phase"
    message_phase = "running"

    def status(self, task_id: str) -> str:
        state = paths.read_implementation_state(task_id)
        phase = state.get("phase")
        if phase in ("running", "done", "error", "stopped"):
            return phase
        # No implementation run yet: awaiting once the plan review is approved.
        from crack_server import stages

        review = stages.get("plan_review")
        if review is not None and review.status(task_id) == "done":
            return "awaiting"
        return "disabled"

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        review = stages.get("plan_review")
        return review is not None and review.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def state_read(self, task_id: str) -> dict:
        return paths.read_implementation_state(task_id)

    def state_write(self, task_id: str, state: dict) -> None:
        paths.write_implementation_state(task_id, state)

    def start(self, task_id: str) -> None:
        state = paths.read_implementation_state(task_id)
        if state.get("phase") == "running":
            return
        shutil.rmtree(paths.implementation_sessions_dir(task_id), ignore_errors=True)
        fresh = {
            "phase": "running",
            "turns": [],
            "current_model": self.model_for("primary"),
            "total_turns": 0,
            "stop_reason": None,
            "error": "",
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        paths.write_implementation_state(task_id, fresh)
        self.enqueue_step(task_id, "run", form)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "run":
            self._run_implementation(task_id)
        elif step == "user_message":
            msg = str((form or {}).get("msg", "")).strip()
            self._run_implementation(
                task_id, initial_message=msg or pi_runner.RESUME_MESSAGE
            )
        else:
            super().run_step(task_id, step, form)

    # -- message assembly -----------------------------------------------------

    def _assemble_handoff(self, task_id: str) -> str:
        final_plan_path = paths.plan_dir(task_id) / "final_plan.md"
        todo_path = paths.plan_todo_path(task_id)
        try:
            final_plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            final_plan = "(no final plan)"
        try:
            todo = paths.read_plan_artefact(task_id, "todo.md")
        except FileNotFoundError:
            todo = "(no todo checklist)"
        explore_summary = paths.read_explore_state(task_id).get("summary_md", "(none)")
        content = paths.read_all_prompts_joined(task_id)

        return (
            self.load_template("handoff.md")
            .replace("{content}", content or "(no prompts)")
            .replace("{explore_summary}", explore_summary)
            .replace("{final_plan}", final_plan)
            .replace("{final_plan_path}", str(final_plan_path))
            .replace("{todo}", todo)
            .replace("{todo_path}", str(todo_path))
            .replace("{walkthrough_path}", str(paths.walkthrough_path(task_id)))
        )

    def _todo_reminder(self, task_id: str) -> str:
        return (
            f"Update your todo file at `{paths.plan_todo_path(task_id)}` to reflect "
            "everything done so far, and reply with its full path. Then keep going."
        )

    def _continue_message(self, task_id: str) -> str:
        return (
            "Continue the implementation. Keep the walkthrough at "
            f"`{paths.walkthrough_path(task_id)}` and the todo at "
            f"`{paths.plan_todo_path(task_id)}` up to date. Emit "
            f"{IMPL_SENTINEL} on its own line when fully done."
        )

    # -- worker step ----------------------------------------------------------

    def _run_implementation(self, task_id: str, initial_message: str | None = None) -> None:
        start = time.monotonic()
        try:
            state = paths.read_implementation_state(task_id)
            current_model = state.get("current_model") or self.model_for("primary")
            if initial_message is not None:
                message, template = initial_message, ""
            else:
                message, template = self._assemble_handoff(task_id), "handoff.md"
            fallback_model = self.model_for("fallback")

            stop_reason = None
            round_n = 0
            while True:
                state = paths.read_implementation_state(task_id)
                existing_turns = list(state.get("turns", []))
                total = pi_runner.count_turn_groups(existing_turns)
                if time.monotonic() - start > IMPL_TIMEOUT_SECONDS:
                    stop_reason = "time_cap"
                    break

                # Every 5 completed turns, nudge the agent to refresh the todo.
                if round_n > 0 and total > 0 and total % IMPL_TODO_REMINDER_EVERY == 0:
                    message, template = self._todo_reminder(task_id), ""

                round_n += 1
                new_turns: list[dict] = []

                def append_entry(entry: dict) -> None:
                    new_turns.append(entry)
                    st = paths.read_implementation_state(task_id)
                    st["turns"] = existing_turns + new_turns
                    st["total_turns"] = len(st["turns"])
                    paths.write_implementation_state(task_id, st)

                def persist(current_turn: dict, hop: int) -> None:
                    append_entry(
                        {
                            "hop": hop,
                            "text": current_turn.get("text", ""),
                            "thinking": current_turn.get("thinking", ""),
                            "tool_blocks": list(current_turn.get("tool_blocks", [])),
                            "elapsed": current_turn.get("elapsed"),
                        }
                    )

                tmpl = template

                def record(entry: dict) -> None:
                    entry.setdefault("label", f"round {round_n}")
                    entry["template"] = tmpl
                    append_entry(entry)

                reason = pi_runner.run_agent_hop(
                    log_prefix="implementation",
                    model=current_model,
                    session_id=f"impl-{task_id}",
                    sessions_dir=paths.implementation_sessions_dir(task_id),
                    tools="bash,read,edit,write,mcp",
                    message=message,
                    start=start,
                    sentinel=IMPL_SENTINEL,
                    timeout_seconds=IMPL_TIMEOUT_SECONDS,
                    persist_turn=persist,
                    hop=round_n,
                    record_prompt=record,
                    **self.agent_hop_kwargs(task_id),
                )

                # Switch to the fallback model only when two adjacent turns fail
                # the same tool the same way (B10) — never on turn counts.
                all_turns = existing_turns + new_turns
                if current_model != fallback_model and _has_consecutive_error(all_turns):
                    current_model = fallback_model
                    st = paths.read_implementation_state(task_id)
                    st["current_model"] = current_model
                    paths.write_implementation_state(task_id, st)
                    logger.info("implementation: switching to fallback model %s", fallback_model)

                if reason == "empty":
                    raise RuntimeError("pi returned empty responses (no content in any turn)")
                if reason == "stopped":
                    st = paths.read_implementation_state(task_id)
                    st["phase"] = "stopped"
                    st["stop_reason"] = "stopped"
                    paths.write_implementation_state(task_id, st)
                    return
                if reason == "sentinel":
                    stop_reason = "sentinel"
                    break
                if reason == "time_cap":
                    stop_reason = "time_cap"
                    break
                # agent_end → keep going with a continuation nudge.
                message, template = self._continue_message(task_id), ""

            state = paths.read_implementation_state(task_id)
            state["phase"] = "done"
            state["stop_reason"] = stop_reason
            state["finished_at"] = time.time()
            paths.write_implementation_state(task_id, state)
            git_utils.commit(paths.task_dir(task_id), f"implementation done {task_id}")
            logger.info(
                "implementation: done for %s stop_reason=%s turns=%d",
                task_id, stop_reason, len(state.get("turns", [])),
            )

            from crack_server import stages

            review = stages.get("impl_review")
            if review is not None:
                review.start(task_id)
        except Exception as e:
            logger.exception("implementation worker failed for %s", task_id)
            state = paths.read_implementation_state(task_id)
            state["phase"] = "error"
            state["error"] = str(e)
            state["error_detail"] = getattr(e, "detail", "")
            state["error_step"] = "run"
            state["finished_at"] = time.time()
            paths.write_implementation_state(task_id, state)

    def retry_from_error(self, task_id: str) -> None:
        """Resume implementation: the run loop reads existing turns and resumes the
        agent's pi session, so it continues from where it crashed."""
        state = paths.read_implementation_state(task_id)
        if state.get("phase") != "error":
            return
        state["phase"] = "running"
        state["error"] = ""
        state["error_detail"] = ""
        paths.write_implementation_state(task_id, state)
        self.enqueue_step(task_id, state.get("error_step") or "run")

    # -- rendering ------------------------------------------------------------

    def render_status(self, task_id: str, oob: bool = False) -> str:
        content_id = self.stage_content_id()
        state = paths.read_implementation_state(task_id)
        phase = state.get("phase")
        turns = state.get("turns", [])
        parts: list[str] = []

        if phase is None:
            if not self.is_enabled(task_id):
                parts.append(
                    '<div class="stage-msg"><p style="color: #888;">'
                    "Approve the plan first to unlock implementation.</p></div>"
                )
            else:
                parts.append(
                    '<div class="stage-msg"><p>Ready to implement the approved plan.</p></div>'
                )
                parts.append(
                    f'<div class="stage-msg"><button hx-post="{self.start_url(task_id)}" '
                    f'hx-target="#{content_id}" hx-swap="outerHTML">Start implementation</button></div>'
                )
            return self.wrap_status(
                task_id, "".join(parts), msg_count=max(len(parts), 1),
                polling=False, extra_class="implementation-content", oob=oob,
            )

        model = state.get("current_model", "")
        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"implemented {_ui._format_ago(finished_at)}" if finished_at else "implemented"
            meta += f" · {len(turns)} turns"
            if state.get("stop_reason"):
                meta += f" · stop: {_esc(str(state['stop_reason']))}"
            parts.append(f'<div class="stage-msg implementation-meta"><small>{meta}</small></div>')

        parts.append(render_turns_trajectory(turns))

        walkthrough = paths.read_walkthrough(task_id)
        if walkthrough:
            parts.append(
                '<div class="stage-msg implementation-walkthrough"><h3>Walkthrough</h3>'
                f"{_ui._render_markdown(walkthrough)}</div>"
            )

        if phase == "error":
            parts.append(
                render_error_msg(state.get("error", ""), state.get("error_detail", ""))
            )

        if phase in ("done", "error", "stopped"):
            buttons = (
                f'<button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">Re-run implementation</button>'
            )
            if phase == "error":
                buttons += render_retry_button(self, task_id, state.get("error_step"))
            parts.append(f'<div class="stage-msg stage-buttons">{buttons}</div>')

        if phase in ("error", "stopped"):
            parts.append(render_message_form(self, task_id))

        if phase == "running":
            parts.append(
                render_spinner(
                    f"Implementing… turn {pi_runner.count_turn_groups(turns)} · model {model}"
                )
            )
            parts.append(render_stop_button(self, task_id))

        msg_count = max(len(turns) + len(parts), 1)
        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=phase == "running",
            extra_class="implementation-content",
            oob=oob,
        )


STAGE = S04Implementation()
