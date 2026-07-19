"""Stage s06: Finished — the walkthrough retrospective, final plan, and review
trajectory, plus a chat box that **resumes the review pi session** (tools enabled)
so the user can keep asking questions or requesting further changes.
"""

from __future__ import annotations

from crack_server import chat_engine, paths
from crack_server.state import JsonState
from crack_server.stages.base import Part, Stage
from crack_server.stages.render import (
    render_error_msg,
    render_exchanges,
    render_running_tail,
    render_turn_msgs,
    render_turns_trajectory,
)
from crack_server import ui as _ui

GLM_MODEL = "nvidia/z-ai/glm-5.2"

CHAT_TIMEOUT_SECONDS = 900



class S06Finished(Stage):
    slug = "finished"
    name = "Finished"
    parts = [
        Part("chat", "Chat (resumes review session)", "chat.md", GLM_MODEL),
    ]

    phase_key = "phase"
    message_phase = "chatting"

    def status(self, task_id: str) -> str:
        phase = paths.finished_state(task_id).read().get("phase")
        if phase == "chatting":
            return "running"
        if phase == "stopped":
            return "stopped"
        from crack_server import stages

        review = stages.get("impl_review")
        if review is not None and review.status(task_id) == "done":
            return "done"
        return "disabled"

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        review = stages.get("impl_review")
        return review is not None and review.status(task_id) == "done"

    # -- chat lifecycle -------------------------------------------------------

    def handle_action(self, action: str, task_id: str, form) -> None:
        # "message" (the generic resume-with-a-message action) and "chat" are
        # the same thing here: both feed the review-session chat.
        if action in ("chat", "message"):
            msg = str(form.get("msg", "")).strip()
            if not msg:
                return
            busy = False

            def _begin(state: dict) -> dict:
                nonlocal busy
                if state.get("phase") == "chatting":
                    # B2: one agent at a time — refuse a concurrent send.
                    busy = True
                    return state
                state.setdefault("exchanges", []).append({"user": msg, "turns": []})
                state["phase"] = "chatting"
                state["stop_requested"] = False
                state.pop("error", None)
                state.pop("error_detail", None)
                return state

            paths.finished_state(task_id).update(_begin)
            if not busy:
                self.enqueue_step(task_id, "chat")
            return
        super().handle_action(action, task_id, form)

    def state(self, task_id: str) -> JsonState:
        return paths.finished_state(task_id)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "chat":
            self._run_chat(task_id)
            return
        super().run_step(task_id, step, form)

    def _run_chat(self, task_id: str) -> None:
        """Thin adapter over chat_engine.run_exchange: resume the review session
        (same session id + dir), tools enabled."""
        chat_engine.run_exchange(
            state=paths.finished_state(task_id),
            ident=task_id,
            message_builder=lambda user_msg: self.load_template("chat.md").replace(
                "{msg}", user_msg
            ),
            record_template="chat.md",
            log_prefix="finished-chat",
            model=self.model_for("chat"),
            session_id=f"review-{task_id}",
            sessions_dir=paths.impl_review_sessions_dir(task_id),
            tools="bash,read,edit,write,mcp",
            timeout_seconds=CHAT_TIMEOUT_SECONDS,
            hop_kwargs=self.agent_hop_kwargs(task_id),
            stopped_phase="stopped",
        )

    # -- rendering ------------------------------------------------------------

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.finished_state(task_id).read()
        phase = state.get("phase")
        msgs: list[str] = []

        if not self.is_enabled(task_id) and phase != "chatting":
            msgs.append(
                '<div class="stage-msg"><p style="color: #888;">'
                "The implementation review must finish before this stage unlocks.</p></div>"
            )
            return msgs

        msgs.append('<div class="stage-msg"><p class="success">All stages complete ✓</p></div>')

        walkthrough = paths.read_walkthrough(task_id)
        if walkthrough:
            msgs.append(
                '<div class="stage-msg finished-walkthrough"><h3>Retrospective / walkthrough</h3>'
                f"{_ui._render_markdown(walkthrough)}</div>"
            )

        try:
            final_plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            final_plan = ""
        if final_plan:
            msgs.append(
                '<details class="stage-msg finished-plan"><summary>Final plan</summary>'
                f"{_ui._render_markdown(final_plan)}</details>"
            )

        review_turns = paths.impl_review_state(task_id).read().get("turns", [])
        if review_turns:
            msgs.append(
                '<details class="stage-msg finished-review"><summary>Review trajectory</summary>'
                f"{render_turns_trajectory(review_turns)}</details>"
            )

        msgs.extend(render_exchanges(state.get("exchanges", []), render_turn_msgs))

        return msgs

    def render_tail(self, task_id: str) -> str:
        content_id = self.stage_content_id()
        target = f"#{content_id}"
        state = paths.finished_state(task_id).read()
        phase = state.get("phase")
        parts: list[str] = []

        if not self.is_enabled(task_id) and phase != "chatting":
            return ""

        if phase != "chatting" and state.get("error"):
            parts.append(render_error_msg(state.get("error", ""), state.get("error_detail", "")))

        if phase == "chatting":
            parts.append(render_running_tail(self, task_id, "Thinking…"))
        else:
            parts.append(f"""
            <form class="finished-chat chat-form" hx-post="{self.action_url(task_id, "chat")}"
                  hx-target="{target}" hx-swap="outerHTML">
              <label>Ask a follow-up or request a change (continues the review session)
                <textarea name="msg" rows="3" required placeholder="Type a message…"></textarea>
              </label>
              <button type="submit">Send</button>
            </form>
            """)

        return "".join(parts)

    def render_status(
        self, task_id: str, oob: bool = False, after: int | None = None
    ) -> str:
        return self.wrap_status(
            task_id,
            self.render_msgs(task_id),
            self.render_tail(task_id),
            after=after,
            extra_class="finished-content",
            oob=oob,
        )


STAGE = S06Finished()
