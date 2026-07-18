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
from crack_server.stages.base import (
    Part,
    Stage,
    render_error_msg,
    render_retry_button,
    render_spinner,
    render_turns_trajectory,
)
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

GLM_MODEL = "nvidia/z-ai/glm-5.2"

REVIEW_SENTINEL = "REVIEW_COMPLETE"
REVIEW_TURNS_PER_HOP = 3
REVIEW_MAX_TURNS = 60
REVIEW_TIMEOUT_SECONDS = 3600


def _esc(text: str) -> str:
    return _ui._esc(text)


class S05ImplReview(Stage):
    slug = "impl_review"
    name = "Implementation Review"
    parts = [
        Part("reviewer", "Reviewer agent", "review.md", GLM_MODEL),
    ]

    def status(self, task_id: str) -> str:
        state = paths.read_impl_review_state(task_id)
        phase = state.get("phase")
        if phase == "running":
            return "running"
        if phase == "done":
            return "done"
        if phase == "error":
            return "error"
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

    def start(self, task_id: str) -> None:
        state = paths.read_impl_review_state(task_id)
        if state.get("phase") == "running":
            return
        shutil.rmtree(paths.impl_review_sessions_dir(task_id), ignore_errors=True)
        paths.write_impl_review_state(
            task_id,
            {
                "phase": "running",
                "turns": [],
                "total_turns": 0,
                "stop_reason": None,
                "error": "",
                "started_at": time.time(),
                "finished_at": None,
            },
        )
        self.enqueue_step(task_id, "run")

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "run":
            self._run_review(task_id)
            return
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
        explore_summary = paths.read_explore_state(task_id).get("summary_md", "(none)")
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

    def _run_review(self, task_id: str) -> None:
        start = time.monotonic()
        try:
            message = self._assemble_message(task_id)
            stop_reason = None
            round_n = 0
            while True:
                state = paths.read_impl_review_state(task_id)
                existing_turns = list(state.get("turns", []))
                total = pi_runner.count_turn_groups(existing_turns)
                if total >= REVIEW_MAX_TURNS:
                    stop_reason = "turn_cap"
                    break
                if time.monotonic() - start > REVIEW_TIMEOUT_SECONDS:
                    stop_reason = "time_cap"
                    break

                round_n += 1
                new_turns: list[dict] = []

                def persist(current_turn: dict, hop: int) -> None:
                    new_turns.append(
                        {
                            "hop": hop,
                            "text": current_turn.get("text", ""),
                            "thinking": current_turn.get("thinking", ""),
                            "tool_blocks": list(current_turn.get("tool_blocks", [])),
                            "elapsed": current_turn.get("elapsed"),
                        }
                    )
                    st = paths.read_impl_review_state(task_id)
                    st["turns"] = existing_turns + new_turns
                    st["total_turns"] = len(st["turns"])
                    paths.write_impl_review_state(task_id, st)

                reason = pi_runner.run_agent_hop(
                    log_prefix="impl-review",
                    model=self.model_for("reviewer"),
                    session_id=f"review-{task_id}",
                    sessions_dir=paths.impl_review_sessions_dir(task_id),
                    tools="bash,read,edit,write,mcp",
                    message=message,
                    start=start,
                    sentinel=REVIEW_SENTINEL,
                    turns_per_hop=REVIEW_TURNS_PER_HOP,
                    max_turns=REVIEW_MAX_TURNS,
                    timeout_seconds=REVIEW_TIMEOUT_SECONDS,
                    total_turns=total,
                    persist_turn=persist,
                    hop=round_n,
                )

                if reason == "empty":
                    raise RuntimeError("pi returned empty responses (no content in any turn)")
                if reason == "sentinel":
                    stop_reason = "sentinel"
                    break
                if reason in ("turn_cap", "time_cap"):
                    stop_reason = reason
                    break
                message = self._continue_message(task_id)

            state = paths.read_impl_review_state(task_id)
            state["phase"] = "done"
            state["stop_reason"] = stop_reason
            state["finished_at"] = time.time()
            paths.write_impl_review_state(task_id, state)
            git_utils.commit(paths.task_dir(task_id), f"review done {task_id}")
            logger.info(
                "impl_review: done for %s stop_reason=%s turns=%d",
                task_id, stop_reason, len(state.get("turns", [])),
            )
        except Exception as e:
            logger.exception("impl_review worker failed for %s", task_id)
            state = paths.read_impl_review_state(task_id)
            state["phase"] = "error"
            state["error"] = str(e)
            state["error_detail"] = getattr(e, "detail", "")
            state["error_step"] = "run"
            state["finished_at"] = time.time()
            paths.write_impl_review_state(task_id, state)

    def retry_from_error(self, task_id: str) -> None:
        """Resume the review: its run loop reads existing turns and resumes the
        reviewer's pi session, continuing from where it crashed."""
        state = paths.read_impl_review_state(task_id)
        if state.get("phase") != "error":
            return
        state["phase"] = "running"
        state["error"] = ""
        state["error_detail"] = ""
        paths.write_impl_review_state(task_id, state)
        self.enqueue_step(task_id, state.get("error_step") or "run")

    # -- rendering ------------------------------------------------------------

    def render_status(self, task_id: str, oob: bool = False) -> str:
        content_id = self.stage_content_id()
        state = paths.read_impl_review_state(task_id)
        phase = state.get("phase")
        turns = state.get("turns", [])
        parts: list[str] = []

        if phase is None:
            if not self.is_enabled(task_id):
                parts.append(
                    '<div class="stage-msg"><p style="color: #888;">'
                    "Finish implementation first to unlock review.</p></div>"
                )
            else:
                parts.append(
                    '<div class="stage-msg"><p>Ready to review the implementation.</p></div>'
                )
                parts.append(
                    f'<div class="stage-msg"><button hx-post="{self.start_url(task_id)}" '
                    f'hx-target="#{content_id}" hx-swap="outerHTML">Start review</button></div>'
                )
            return self.wrap_status(
                task_id, "".join(parts), msg_count=max(len(parts), 1),
                polling=False, extra_class="impl-review-content", oob=oob,
            )

        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"reviewed {_ui._format_ago(finished_at)}" if finished_at else "reviewed"
            meta += f" · {len(turns)} turns"
            if state.get("stop_reason"):
                meta += f" · stop: {_esc(str(state['stop_reason']))}"
            parts.append(f'<div class="stage-msg impl-review-meta"><small>{meta}</small></div>')

        parts.append(render_turns_trajectory(turns))

        walkthrough = paths.read_walkthrough(task_id)
        if walkthrough:
            parts.append(
                '<div class="stage-msg impl-review-walkthrough"><h3>Walkthrough</h3>'
                f"{_ui._render_markdown(walkthrough)}</div>"
            )

        if phase == "error":
            parts.append(
                render_error_msg(state.get("error", ""), state.get("error_detail", ""))
            )

        if phase in ("done", "error"):
            buttons = (
                f'<button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">Re-run review</button>'
            )
            if phase == "error":
                buttons += render_retry_button(self, task_id, state.get("error_step"))
            parts.append(f'<div class="stage-msg stage-buttons">{buttons}</div>')

        if phase == "running":
            parts.append(render_spinner(f"Reviewing… {pi_runner.count_turn_groups(turns)}/{REVIEW_MAX_TURNS} turns"))

        msg_count = max(len(turns) + len(parts), 1)
        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=phase == "running",
            extra_class="impl-review-content",
            oob=oob,
        )


STAGE = S05ImplReview()
