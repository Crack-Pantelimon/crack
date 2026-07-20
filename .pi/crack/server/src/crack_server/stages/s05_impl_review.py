"""Stage s05: Implementation Review — a critic agent that runs `git diff`,
builds/tests everything, fixes wrong code, and loops on any warning or failure
until the work is clean. Runs on the worker like every other agent stage; on
completion the Finished stage unlocks.
"""

from __future__ import annotations

import logging
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

GLM_MODEL = "nvidia/z-ai/glm-5.2"

REVIEW_SENTINEL = "REVIEW_COMPLETE"
REVIEW_TIMEOUT_SECONDS = 3600



class S05ImplReview(Stage):
    slug = "impl_review"
    name = "Implementation Review"
    parts = [
        Part("reviewer", "Reviewer agent", "review.md", GLM_MODEL),
    ]

    phase_key = "phase"
    message_phase = "running"

    def status(self, task_id: str) -> str:
        state = paths.impl_review_state(task_id).read()
        phase = state.get("phase")
        if phase in ("running", "done", "error", "stopped"):
            return phase
        from crack_server import stages

        impl = stages.get("implementation")
        if impl is not None and impl.status(task_id) == "done":
            return "awaiting"
        return "disabled"

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        impl = stages.get("implementation")
        return impl is not None and impl.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def state(self, task_id: str) -> JsonState:
        return paths.impl_review_state(task_id)

    def start(self, task_id: str) -> None:
        review = paths.impl_review_state(task_id)
        if review.read().get("phase") == "running":
            return
        shutil.rmtree(paths.impl_review_sessions_dir(task_id), ignore_errors=True)
        fresh = {
            "phase": "running",
            "turns": [],
            "errors": [],
            "error_budget": pi_runner.MAX_TOTAL_ERRORS,
            "total_turns": 0,
            "stop_reason": None,
            "error": "",
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        review.write(fresh)
        self.enqueue_step(task_id, "run", form)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "run":
            self._run_review(task_id)
        elif step == "user_message":
            msg = str((form or {}).get("msg", "")).strip()
            self._run_review(task_id, initial_message=msg or pi_runner.RESUME_MESSAGE)
        else:
            super().run_step(task_id, step, form)

    # -- message assembly -----------------------------------------------------

    def _assemble_message(self, task_id: str) -> str:
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
        walkthrough = paths.read_walkthrough(task_id) or "(no walkthrough yet)"

        return (
            self.load_template("review.md")
            .replace("{content}", content or "(no prompts)")
            .replace("{explore_summary}", explore_summary)
            .replace("{final_plan}", final_plan)
            .replace("{final_plan_path}", str(final_plan_path))
            .replace("{todo}", todo)
            .replace("{todo_path}", str(todo_path))
            .replace("{walkthrough_path}", str(paths.walkthrough_path(task_id)))
            .replace("{walkthrough}", walkthrough)
        )

    def _continue_message(self, task_id: str) -> str:
        return (
            "Continue the review. Keep building/testing and fixing until nothing "
            f"is failing, updating `{paths.walkthrough_path(task_id)}`. Emit "
            f"{REVIEW_SENTINEL} on its own line when everything is clean."
        )

    # -- worker step ----------------------------------------------------------

    def _run_review(self, task_id: str, initial_message: str | None = None) -> None:
        start = time.monotonic()
        review = paths.impl_review_state(task_id)
        with record_errors(review, "run", log_message=f"impl_review worker failed for {task_id}"):
            if initial_message is not None:
                message, init_template = initial_message, ""
            else:
                message, init_template = self._assemble_message(task_id), "review.md"

            def run_hop(msg: str, round_n: int) -> str:
                persister = turn_persister(review, post=bump_total_turns, media_dir=paths.task_dir(task_id) / "media", media_url_prefix=f"/tasks/{task_id}/media")
                tmpl = init_template if round_n == 1 else ""
                return pi_runner.run_agent_hop(
                    log_prefix="impl-review",
                    model=self.model_for("reviewer"),
                    session_id=f"review-{task_id}",
                    sessions_dir=paths.impl_review_sessions_dir(task_id),
                    tools="bash,read,edit,write,mcp,analyze_image",
                    message=msg,
                    start=start,
                    sentinel=REVIEW_SENTINEL,
                    timeout_seconds=REVIEW_TIMEOUT_SECONDS,
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
                timeout_seconds=REVIEW_TIMEOUT_SECONDS,
                message=message,
                run_hop=run_hop,
                continue_message=lambda: self._continue_message(task_id),
                on_stopped=lambda: mark_run_stopped(review),
            )
            if stop_reason is None:  # externally stopped; state already written
                return

            def _finish(state: dict) -> dict:
                state["phase"] = "done"
                state["stop_reason"] = stop_reason
                state["finished_at"] = time.time()
                return state

            state = review.update(_finish)
            git_utils.commit(paths.task_dir(task_id), f"review done {task_id}")
            logger.info(
                "impl_review: done for %s stop_reason=%s turns=%d",
                task_id, stop_reason, len(state.get("turns", [])),
            )

    def retry_from_error(self, task_id: str) -> None:
        """Resume the review: its run loop reads existing turns and resumes the
        reviewer's pi session, continuing from where it crashed."""
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

        paths.impl_review_state(task_id).update(_retry)
        if retry:
            self.enqueue_step(task_id, step)

    # -- rendering ------------------------------------------------------------

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.impl_review_state(task_id).read()
        phase = state.get("phase")
        turns = state.get("turns", [])
        msgs: list[str] = []

        if phase is None:
            if not self.is_enabled(task_id):
                msgs.append(
                    '<div class="stage-msg"><p class="muted">'
                    "Finish implementation first to unlock review.</p></div>"
                )
            else:
                msgs.append(
                    '<div class="stage-msg"><p>Ready to review the implementation.</p></div>'
                )
            return msgs

        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"reviewed {_ui._format_ago(finished_at)}" if finished_at else "reviewed"
            meta += f" · {len(turns)} turns"
            if state.get("stop_reason"):
                meta += f" · stop: {_esc(str(state['stop_reason']))}"
            msgs.append(f'<div class="stage-msg impl-review-meta"><small>{meta}</small></div>')

        msgs.extend(render_turn_msgs(turns, errors=state.get("errors", [])))

        if phase == "done":
            walkthrough = paths.read_walkthrough(task_id)
            if walkthrough:
                msgs.append(
                    '<div class="stage-msg impl-review-walkthrough"><h3>Walkthrough</h3>'
                    f"{_ui._render_markdown(walkthrough)}</div>"
                )

        return msgs

    def render_tail(self, task_id: str) -> str:
        content_id = self.stage_content_id()
        state = paths.impl_review_state(task_id).read()
        phase = state.get("phase")
        turns = state.get("turns", [])
        parts: list[str] = []

        if phase is None:
            if self.is_enabled(task_id):
                parts.append(
                    f'<div class="stage-buttons"><button hx-post="{self.start_url(task_id)}" '
                    f'hx-target="#{content_id}" hx-swap="outerHTML">Start review</button></div>'
                )
            return "".join(parts)

        if phase != "done":
            walkthrough = paths.read_walkthrough(task_id)
            if walkthrough:
                parts.append(
                    '<div class="impl-review-walkthrough"><h3>Walkthrough</h3>'
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
                f'hx-target="#{content_id}" hx-swap="outerHTML">Re-run review</button>'
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
                    f"Reviewing… turn {pi_runner.count_turn_groups(turns)}",
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
            extra_class="impl-review-content",
            oob=oob,
        )


STAGE = S05ImplReview()
