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
from crack_server.state import JsonState
from crack_server.stages.base import Part, Stage
from crack_server.stages.render import (
    render_error_msg,
    render_fatal_error_banner,
    render_message_form,
    render_retry_button,
    render_running_tail,
    render_turn_msgs,
)
from crack_server import ui as _ui
from crack_server.ui import _esc
from crack_server.stages.steprun import (
    bump_total_turns,
    grant_error_budget,
    hop_loop,
    mark_run_stopped,
    prompt_recorder,
    record_errors,
    task_prompt_media,
    turn_persister,
)

logger = logging.getLogger("uvicorn.error")

KIMI_MODEL = "nvidia/moonshotai/kimi-k2.6"
GLM_MODEL = "nvidia/z-ai/glm-5.2"

IMPL_SENTINEL = "IMPLEMENTATION_COMPLETE"
IMPL_TIMEOUT_SECONDS = 7200
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
        state = paths.implementation_state(task_id).read()
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

    def state(self, task_id: str) -> JsonState:
        return paths.implementation_state(task_id)

    def start(self, task_id: str) -> None:
        impl = paths.implementation_state(task_id)
        if impl.read().get("phase") == "running":
            return
        shutil.rmtree(paths.implementation_sessions_dir(task_id), ignore_errors=True)
        fresh = {
            "phase": "running",
            "turns": [],
            "errors": [],
            "error_budget": pi_runner.MAX_TOTAL_ERRORS,
            "current_model": self.model_for("primary"),
            "total_turns": 0,
            "stop_reason": None,
            "error": "",
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        impl.write(fresh)
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
        explore_summary = paths.explore_state(task_id).read().get("summary_md", "(none)")
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
        impl = paths.implementation_state(task_id)
        with record_errors(impl, "run", log_message=f"implementation worker failed for {task_id}"):
            current_model = impl.read().get("current_model") or self.model_for("primary")
            if initial_message is not None:
                message, init_template = initial_message, ""
            else:
                message, init_template = self._assemble_handoff(task_id), "handoff.md"
            fallback_model = self.model_for("fallback")

            # Every 5 completed turns, nudge the agent to refresh the todo.
            def before_round(round_n: int) -> str | None:
                if round_n > 1:
                    total = pi_runner.count_turn_groups(impl.read().get("turns", []))
                    if total > 0 and total % IMPL_TODO_REMINDER_EVERY == 0:
                        return self._todo_reminder(task_id)
                return None

            # Switch to the fallback model only when two adjacent turns fail
            # the same tool the same way (B10) — never on turn counts.
            def after_hop(reason: str, round_n: int) -> None:
                nonlocal current_model
                if current_model != fallback_model and _has_consecutive_error(
                    impl.read().get("turns", [])
                ):
                    current_model = fallback_model

                    def _switch_model(state: dict) -> dict:
                        state["current_model"] = current_model
                        return state

                    impl.update(_switch_model)
                    logger.info("implementation: switching to fallback model %s", fallback_model)

            def run_hop(msg: str, round_n: int) -> str:
                persister = turn_persister(impl, post=bump_total_turns, media_dir=paths.task_dir(task_id) / "media", media_url_prefix=f"/tasks/{task_id}/media")
                tmpl = init_template if round_n == 1 else ""
                return pi_runner.run_agent_hop(
                    log_prefix="implementation",
                    model=current_model,
                    session_id=f"impl-{task_id}",
                    sessions_dir=paths.implementation_sessions_dir(task_id),
                    tools="bash,read,edit,write,mcp,analyze_image",
                    message=msg,
                    start=start,
                    sentinel=IMPL_SENTINEL,
                    timeout_seconds=IMPL_TIMEOUT_SECONDS,
                    persist_turn=persister.persist,
                    hop=round_n,
                    record_prompt=prompt_recorder(
                        persister, f"round {round_n}", tmpl,
                        media=lambda: task_prompt_media(task_id),
                    ),
                    **self.agent_hop_kwargs(task_id),
                )

            stop_reason = hop_loop(
                start=start,
                timeout_seconds=IMPL_TIMEOUT_SECONDS,
                message=message,
                run_hop=run_hop,
                continue_message=lambda: self._continue_message(task_id),
                before_round=before_round,
                after_hop=after_hop,
                on_stopped=lambda: mark_run_stopped(impl),
            )
            if stop_reason is None:  # externally stopped; state already written
                return

            def _finish(state: dict) -> dict:
                state["phase"] = "done"
                state["stop_reason"] = stop_reason
                state["finished_at"] = time.time()
                return state

            state = impl.update(_finish)
            git_utils.commit(paths.task_dir(task_id), f"implementation done {task_id}")
            logger.info(
                "implementation: done for %s stop_reason=%s turns=%d",
                task_id, stop_reason, len(state.get("turns", [])),
            )

            from crack_server import stages

            review = stages.get("impl_review")
            if review is not None:
                review.start(task_id)

    def retry_from_error(self, task_id: str) -> None:
        """Resume implementation: the run loop reads existing turns and resumes the
        agent's pi session, so it continues from where it crashed."""
        retry = False
        step = "run"

        def _retry(state: dict) -> dict:
            nonlocal retry, step
            if state.get("phase") != "error":
                return state
            retry = True
            step = state.get("error_step") or "run"
            state["phase"] = "running"
            state["error"] = ""
            state["error_detail"] = ""
            grant_error_budget(state)
            return state

        paths.implementation_state(task_id).update(_retry)
        if retry:
            self.enqueue_step(task_id, step)

    # -- rendering ------------------------------------------------------------

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.implementation_state(task_id).read()
        phase = state.get("phase")
        turns = state.get("turns", [])
        msgs: list[str] = []

        if phase is None:
            if not self.is_enabled(task_id):
                msgs.append(
                    '<div class="stage-msg"><p class="muted">'
                    "Approve the plan first to unlock implementation.</p></div>"
                )
            else:
                msgs.append(
                    '<div class="stage-msg"><p>Ready to implement the approved plan.</p></div>'
                )
            return msgs

        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"implemented {_ui._format_ago(finished_at)}" if finished_at else "implemented"
            meta += f" · {len(turns)} turns"
            if state.get("stop_reason"):
                meta += f" · stop: {_esc(str(state['stop_reason']))}"
            msgs.append(f'<div class="stage-msg implementation-meta"><small>{meta}</small></div>')

        msgs.extend(render_turn_msgs(turns, errors=state.get("errors", [])))

        if phase == "done":
            walkthrough = paths.read_walkthrough(task_id)
            if walkthrough:
                msgs.append(
                    '<div class="stage-msg implementation-walkthrough"><h3>Walkthrough</h3>'
                    f"{_ui._render_markdown(walkthrough)}</div>"
                )

        return msgs

    def render_tail(self, task_id: str) -> str:
        content_id = self.stage_content_id()
        state = paths.implementation_state(task_id).read()
        phase = state.get("phase")
        turns = state.get("turns", [])
        parts: list[str] = []

        if phase is None:
            if self.is_enabled(task_id):
                parts.append(
                    f'<div class="stage-buttons"><button hx-post="{self.start_url(task_id)}" '
                    f'hx-target="#{content_id}" hx-swap="outerHTML">Start implementation</button></div>'
                )
            return "".join(parts)

        model = state.get("current_model", "")

        # Walkthrough mutates during the run — keep it in the tail until done.
        if phase != "done":
            walkthrough = paths.read_walkthrough(task_id)
            if walkthrough:
                parts.append(
                    '<div class="implementation-walkthrough"><h3>Walkthrough</h3>'
                    f"{_ui._render_markdown(walkthrough)}</div>"
                )

        if phase == "error":
            parts.append(render_fatal_error_banner(state))
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
            parts.append(f'<div class="stage-buttons">{buttons}</div>')

        if phase in ("error", "stopped"):
            parts.append(render_message_form(self, task_id))

        if phase == "running":
            parts.append(
                render_running_tail(
                    self,
                    task_id,
                    f"Implementing… turn {pi_runner.count_turn_groups(turns)} · model {model}",
                )
            )

        return "".join(parts)

    def render_status(
        self, task_id: str, oob: bool = False, after: int | None = None
    ) -> str:
        return self.wrap_status(
            task_id,
            self.render_msgs(task_id),
            self.render_tail(task_id),
            after=after,
            extra_class="implementation-content",
            oob=oob,
        )


STAGE = S04Implementation()
