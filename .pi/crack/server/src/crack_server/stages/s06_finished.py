"""Stage s06: Finished — the walkthrough retrospective, final plan, and review
trajectory, plus a chat box that **resumes the review pi session** (tools enabled)
so the user can keep asking questions or requesting further changes.
"""

from __future__ import annotations

import logging
import time

from crack_server import paths, pi_runner
from crack_server.stages.base import (
    Part,
    Stage,
    render_error_msg,
    render_spinner,
    render_stop_button,
    render_turns_trajectory,
)
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

GLM_MODEL = "nvidia/z-ai/glm-5.2"

CHAT_TIMEOUT_SECONDS = 900


def _esc(text: str) -> str:
    return _ui._esc(text)


class S06Finished(Stage):
    slug = "finished"
    name = "Finished"
    parts = [
        Part("chat", "Chat (resumes review session)", "chat.md", GLM_MODEL),
    ]

    phase_key = "phase"
    message_phase = "chatting"

    def status(self, task_id: str) -> str:
        phase = paths.read_finished_state(task_id).get("phase")
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
            state = paths.read_finished_state(task_id)
            if state.get("phase") == "chatting":
                # B2: one agent at a time — refuse a concurrent send.
                return
            exchanges = state.setdefault("exchanges", [])
            exchanges.append({"user": msg, "turns": []})
            state["phase"] = "chatting"
            state["stop_requested"] = False
            state.pop("error", None)
            state.pop("error_detail", None)
            paths.write_finished_state(task_id, state)
            self.enqueue_step(task_id, "chat")
            return
        super().handle_action(action, task_id, form)

    def state_read(self, task_id: str) -> dict:
        return paths.read_finished_state(task_id)

    def state_write(self, task_id: str, state: dict) -> None:
        paths.write_finished_state(task_id, state)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "chat":
            self._run_chat(task_id)
            return
        super().run_step(task_id, step, form)

    def _run_chat(self, task_id: str) -> None:
        start = time.monotonic()
        try:
            state = paths.read_finished_state(task_id)
            exchanges = state.get("exchanges", [])
            if not exchanges:
                return
            idx = len(exchanges) - 1
            user_msg = exchanges[idx].get("user", "")
            message = self.load_template("chat.md").replace("{msg}", user_msg)

            existing = list(exchanges[idx].get("turns", []))
            new_turns: list[dict] = []

            def append_entry(entry: dict) -> None:
                new_turns.append(entry)
                st = paths.read_finished_state(task_id)
                st["exchanges"][idx]["turns"] = existing + new_turns
                paths.write_finished_state(task_id, st)

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

            def record(entry: dict) -> None:
                entry["label"] = "chat"
                entry["template"] = "chat.md"
                # The compiled message was built from this single user message.
                entry["original"] = user_msg
                append_entry(entry)

            # Resume the review session (same session id + dir), tools enabled.
            reason = pi_runner.run_agent_hop(
                log_prefix="finished-chat",
                model=self.model_for("chat"),
                session_id=f"review-{task_id}",
                sessions_dir=paths.impl_review_sessions_dir(task_id),
                tools="bash,read,edit,write,mcp",
                message=message,
                start=start,
                sentinel=None,
                timeout_seconds=CHAT_TIMEOUT_SECONDS,
                persist_turn=persist,
                hop=1,
                record_prompt=record,
                **self.agent_hop_kwargs(task_id),
            )

            state = paths.read_finished_state(task_id)
            state["phase"] = "stopped" if reason == "stopped" else "idle"
            state["stop_requested"] = False
            if reason == "empty":
                state["error"] = "model returned empty responses"
                state["error_detail"] = ""
            paths.write_finished_state(task_id, state)
            logger.info("finished: chat exchange %d done for %s (reason=%s)", idx, task_id, reason)
        except Exception as e:
            logger.exception("finished chat failed for %s", task_id)
            state = paths.read_finished_state(task_id)
            state["phase"] = "idle"
            # A kill triggered by STOP surfaces as an exception on some paths;
            # don't dress an intentional halt up as an error.
            if not state.get("stop_requested"):
                state["error"] = str(e)
                state["error_detail"] = getattr(e, "detail", "")
            state["stop_requested"] = False
            paths.write_finished_state(task_id, state)

    # -- rendering ------------------------------------------------------------

    def render_status(self, task_id: str, oob: bool = False) -> str:
        content_id = self.stage_content_id()
        target = f"#{content_id}"
        state = paths.read_finished_state(task_id)
        phase = state.get("phase")
        parts: list[str] = []

        if not self.is_enabled(task_id) and phase != "chatting":
            parts.append(
                '<div class="stage-msg"><p style="color: #888;">'
                "The implementation review must finish before this stage unlocks.</p></div>"
            )
            return self.wrap_status(
                task_id, "".join(parts), msg_count=1,
                polling=False, extra_class="finished-content", oob=oob,
            )

        parts.append('<div class="stage-msg"><p class="success">All stages complete ✓</p></div>')

        walkthrough = paths.read_walkthrough(task_id)
        if walkthrough:
            parts.append(
                '<div class="stage-msg finished-walkthrough"><h3>Retrospective / walkthrough</h3>'
                f"{_ui._render_markdown(walkthrough)}</div>"
            )

        try:
            final_plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            final_plan = ""
        if final_plan:
            parts.append(
                '<details class="stage-msg finished-plan"><summary>Final plan</summary>'
                f"{_ui._render_markdown(final_plan)}</details>"
            )

        review_turns = paths.read_impl_review_state(task_id).get("turns", [])
        if review_turns:
            parts.append(
                '<details class="stage-msg finished-review"><summary>Review trajectory</summary>'
                f"{render_turns_trajectory(review_turns)}</details>"
            )

        # Chat exchanges (each: user bubble + resumed-session agent turns).
        for exchange in state.get("exchanges", []):
            parts.append(
                '<div class="stage-msg chat-user"><strong>You:</strong> '
                f"{_esc(exchange.get('user', ''))}</div>"
            )
            turns = exchange.get("turns", [])
            if turns:
                parts.append(render_turns_trajectory(turns))

        if phase != "chatting" and state.get("error"):
            parts.append(render_error_msg(state.get("error", ""), state.get("error_detail", "")))

        if phase == "chatting":
            parts.append(render_spinner("Thinking…"))
            parts.append(render_stop_button(self, task_id))
        else:
            parts.append(f"""
            <form class="stage-msg finished-chat" hx-post="{self.action_url(task_id, "chat")}"
                  hx-target="{target}" hx-swap="outerHTML">
              <label>Ask a follow-up or request a change (continues the review session)
                <textarea name="msg" rows="3" required placeholder="Type a message…"></textarea>
              </label>
              <button type="submit">Send</button>
            </form>
            """)

        msg_count = max(len(parts) + len(state.get("exchanges", [])), 1)
        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=phase == "chatting",
            extra_class="finished-content",
            oob=oob,
        )


STAGE = S06Finished()
