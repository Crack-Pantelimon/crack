"""Stage s03: Plan Review — glm critic grills the user, edits final_plan.md in place,
regenerates todo.md, and gates on Approve / Reject / Grill-more."""

from __future__ import annotations

import json
import logging
import re
import shutil
import time

from crack_server import git_utils, paths, pi_runner
from crack_server.stages.base import (
    Part,
    Stage,
    collect_answers,
    format_qa_for_prompt,
    parse_questions,
    render_error_msg,
    render_message_form,
    render_qa_history,
    render_questions_form,
    render_retry_button,
    render_spinner,
    render_stop_button,
    render_turns_trajectory,
)
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

GLM_MODEL = "nvidia/z-ai/glm-5.2"
ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

MAX_AUTO_ROUNDS = 2
PLAN_REVISED_SENTINEL = "PLAN_REVISED"
READY_TO_REVISE = "READY_TO_REVISE"
RUNNING_PHASES = ("review_running", "resuming", "revising")
CRITIC_TIMEOUT_SECONDS = 300

# Flow-control nudge (not a cap): sent once when the critic ended its turn
# without emitting either a questions block or a sentinel.
CRITIC_NUDGE = (
    "Stop calling tools now. Complete your response — emit either a "
    "```questions JSON block or PLAN_REVISED on its own line."
)


def _esc(text: str) -> str:
    return _ui._esc(text)


class S03PlanReview(Stage):
    slug = "plan_review"
    name = "Plan Review"
    parts = [
        Part("critic", "Plan critic (Q&A + edits)", "critique.md", GLM_MODEL),
        Part("todo", "Todo generator (single-shot)", "todo.md", ULTRA_MODEL),
    ]

    phase_key = "phase"
    message_phase = "resuming"

    def status(self, task_id: str) -> str:
        phase = paths.read_plan_review_state(task_id).get("phase", "idle")
        if phase in RUNNING_PHASES:
            return "running"
        if phase in ("awaiting_answers", "awaiting_approval"):
            return "awaiting"
        if phase in ("done", "error", "stopped"):
            return phase
        return "idle"

    def state_read(self, task_id: str) -> dict:
        return paths.read_plan_review_state(task_id)

    def state_write(self, task_id: str, state: dict) -> None:
        paths.write_plan_review_state(task_id, state)

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        plan = stages.get("plan")
        return plan is not None and plan.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") in RUNNING_PHASES:
            return

        try:
            plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            paths.write_plan_review_state(
                task_id, {"phase": "error", "error": "no final_plan.md — run Plan first"}
            )
            return

        shutil.rmtree(paths.plan_review_sessions_dir(task_id), ignore_errors=True)

        fresh = {
            "phase": "review_running",
            "round": 1,
            "rounds": [],
            "turns": [],
            "plan_md": plan_md,
            "iterations": 0,
            "error": "",
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        paths.write_plan_review_state(task_id, fresh)
        self.enqueue_step(task_id, "critique", form)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step in ("critique", "followup", "grill", "revise", "reject", "user_message"):
            self._run_review_step(task_id, step, form)
            return
        super().run_step(task_id, step, form)

    def handle_action(self, action: str, task_id: str, form) -> None:
        if action == "answers":
            self._submit_answers(task_id, form)
        elif action == "approve":
            self._approve(task_id)
        elif action == "reject":
            reason = str(form.get("reason", "")).strip()
            if reason:
                self._reject(task_id, reason)
        elif action == "grill":
            topic = str(form.get("topic", "")).strip()
            if topic:
                self._grill_more(task_id, topic)
        else:
            super().handle_action(action, task_id, form)

    def regenerate_todo(self, task_id: str) -> None:
        """Tool-less single-shot: rewrite plan/todo.md from current final_plan.md."""
        try:
            plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            logger.warning("plan_review: no final_plan.md for todo regen on %s", task_id)
            return
        prompt = self.load_template("todo.md").replace("{plan}", plan)

        def record(entry: dict) -> None:
            entry["label"] = "todo"
            entry["template"] = "todo.md"
            state = paths.read_plan_review_state(task_id)
            state.setdefault("turns", []).append(entry)
            paths.write_plan_review_state(task_id, state)

        todo_md, _ = pi_runner.run_pi_text(
            prompt,
            log_prefix="plan-review-todo",
            model=self.model_for("todo"),
            record_prompt=record,
        )
        paths.write_plan_artefact(task_id, "todo.md", todo_md)
        logger.info("plan_review: regenerated todo.md for %s (%d chars)", task_id, len(todo_md))

    # -- action handlers ------------------------------------------------------

    def _submit_answers(self, task_id: str, form) -> None:
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") != "awaiting_answers" or not state.get("rounds"):
            return
        current = state["rounds"][-1]
        current["answers"] = collect_answers(form, current.get("questions", []))
        rnd = int(state.get("round", 1))
        paths.write_plan_artefact(
            task_id, f"review_round_{rnd}_answers.json",
            json.dumps(current["answers"], indent=2),
        )
        state["phase"] = "resuming"
        paths.write_plan_review_state(task_id, state)
        self.enqueue_step(task_id, "followup")

    def _approve(self, task_id: str) -> None:
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") != "awaiting_approval":
            return
        state["phase"] = "done"
        state["finished_at"] = time.time()
        paths.write_plan_review_state(task_id, state)
        logger.info("plan_review: approved for %s", task_id)

        git_utils.commit(paths.task_dir(task_id), f"plan approved {task_id}")
        from crack_server import stages

        impl = stages.get("implementation")
        if impl is not None:
            impl.start(task_id)

    def _reject(self, task_id: str, reason: str) -> None:
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") != "awaiting_approval":
            return
        state["phase"] = "revising"
        state["reject_reason"] = reason
        paths.write_plan_review_state(task_id, state)
        self.enqueue_step(task_id, "reject")

    def _grill_more(self, task_id: str, topic: str) -> None:
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") != "awaiting_approval":
            return
        state["phase"] = "resuming"
        state["grill_topic"] = topic
        paths.write_plan_review_state(task_id, state)
        self.enqueue_step(task_id, "grill")

    # -- background steps -------------------------------------------------------

    def _run_critic_hop(
        self,
        task_id: str,
        message: str,
        *,
        tools: str,
        log_suffix: str,
        template: str = "",
    ) -> tuple[str, list[dict], str]:
        """Run one critic step via pi session; return combined text, new turns,
        and the hop's stop reason (callers must handle "stopped")."""
        start = time.monotonic()
        state = paths.read_plan_review_state(task_id)
        existing_turns = list(state.get("turns", []))
        new_turns: list[dict] = []

        def append_entry(entry: dict) -> None:
            new_turns.append(entry)
            st = paths.read_plan_review_state(task_id)
            st["turns"] = existing_turns + new_turns
            paths.write_plan_review_state(task_id, st)

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

        def hop_once(msg: str, tmpl: str, hop: int) -> str:
            def record(entry: dict) -> None:
                entry.setdefault("label", log_suffix)
                entry["template"] = tmpl
                append_entry(entry)

            return pi_runner.run_agent_hop(
                log_prefix=f"plan-review-{log_suffix}",
                model=self.model_for("critic"),
                session_id=f"plan-review-{task_id}",
                sessions_dir=paths.plan_review_sessions_dir(task_id),
                tools=tools,
                message=msg,
                start=start,
                sentinel=None,
                timeout_seconds=CRITIC_TIMEOUT_SECONDS,
                persist_turn=persist,
                hop=hop,
                record_prompt=record,
                **self.agent_hop_kwargs(task_id),
            )

        reason = hop_once(message, template, 1)
        if reason == "empty":
            raise RuntimeError("pi returned empty responses (no content in any turn)")
        text = "\n\n".join(t["text"] for t in new_turns if t.get("text")).strip()

        # One flow-control nudge (not a cap) when the critic ended without a
        # questions block or a sentinel.
        if (
            reason == "agent_end"
            and not parse_questions(text)
            and PLAN_REVISED_SENTINEL not in text
            and READY_TO_REVISE not in text
        ):
            reason = hop_once(CRITIC_NUDGE, "", 2)
            if reason == "empty":
                raise RuntimeError("pi returned empty responses (no content in any turn)")
            text = "\n\n".join(t["text"] for t in new_turns if t.get("text")).strip()

        return text, new_turns, reason

    def _run_review_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        try:
            state = paths.read_plan_review_state(task_id)

            if step == "critique":
                plan_md = state.get("plan_md") or paths.read_plan_artefact(task_id, "final_plan.md")
                message = (
                    self.load_template("critique.md")
                    .replace("{content}", paths.read_all_prompts_joined(task_id))
                    .replace(
                        "{explore_summary}",
                        paths.read_explore_state(task_id).get("summary_md")
                        or "(no exploration summary)",
                    )
                    .replace("{plan}", plan_md)
                )
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp", log_suffix="critique",
                    template="critique.md",
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                questions = parse_questions(text)
                if not questions:
                    questions = [
                        {
                            "id": "q1",
                            "text": "Does this plan cover everything you need?",
                            "type": "single",
                            "options": ["Yes, proceed", "No, I have concerns"],
                        }
                    ]
                state = paths.read_plan_review_state(task_id)
                state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                state["phase"] = "awaiting_answers"
                paths.write_plan_review_state(task_id, state)
                rnd = int(state.get("round", 1))
                paths.write_plan_artefact(
                    task_id, f"review_round_{rnd}_questions.json",
                    json.dumps(questions, indent=2),
                )
                return

            if step == "followup":
                rnd = int(state.get("round", 1))
                qa_all = "\n\n".join(
                    f"Round {i}:\n{format_qa_for_prompt(r)}"
                    for i, r in enumerate(state.get("rounds", []), 1)
                )
                message = self.load_template("grill_followup.md").replace("{qa}", qa_all)
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp", log_suffix="followup",
                    template="grill_followup.md",
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                questions = parse_questions(text)
                if questions and rnd < MAX_AUTO_ROUNDS and READY_TO_REVISE not in text:
                    state = paths.read_plan_review_state(task_id)
                    state["round"] = rnd + 1
                    state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                    state["phase"] = "awaiting_answers"
                    paths.write_plan_review_state(task_id, state)
                    paths.write_plan_artefact(
                        task_id, f"review_round_{rnd + 1}_questions.json",
                        json.dumps(questions, indent=2),
                    )
                    return
                state = paths.read_plan_review_state(task_id)
                state["phase"] = "revising"
                paths.write_plan_review_state(task_id, state)
                self.enqueue_step(task_id, "revise")
                return

            if step == "grill":
                topic = state.get("grill_topic", "")
                message = (
                    f"The user wants to grill the plan further on this topic:\n{topic}\n\n"
                    "Emit a ```questions JSON block with at most 5 clarifying questions."
                )
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp", log_suffix="grill"
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                questions = parse_questions(text)
                if not questions:
                    questions = [
                        {
                            "id": "q1",
                            "text": topic,
                            "type": "open",
                        }
                    ]
                state = paths.read_plan_review_state(task_id)
                rnd = int(state.get("round", 1)) + 1
                state["round"] = rnd
                state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                state["phase"] = "awaiting_answers"
                state.pop("grill_topic", None)
                paths.write_plan_review_state(task_id, state)
                paths.write_plan_artefact(
                    task_id, f"review_round_{rnd}_questions.json",
                    json.dumps(questions, indent=2),
                )
                return

            if step in ("revise", "reject"):
                plan_path = paths.plan_dir(task_id) / "final_plan.md"
                if step == "reject":
                    reason = state.get("reject_reason", "")
                    message = (
                        self.load_template("reject.md")
                        .replace("{reason}", reason)
                        .replace("{plan_path}", str(plan_path))
                    )
                    state.pop("reject_reason", None)
                else:
                    message = self.load_template("revise.md").replace(
                        "{plan_path}", str(plan_path)
                    )
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,edit,write,mcp", log_suffix=step,
                    template=f"{step}.md",
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                if PLAN_REVISED_SENTINEL not in text:
                    logger.warning("plan_review: critic did not emit PLAN_REVISED; continuing")

                plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
                state = paths.read_plan_review_state(task_id)
                state["plan_md"] = plan_md
                state["iterations"] = int(state.get("iterations", 0)) + 1
                paths.write_plan_review_state(task_id, state)
                self.regenerate_todo(task_id)

                state = paths.read_plan_review_state(task_id)
                state["phase"] = "awaiting_approval"
                paths.write_plan_review_state(task_id, state)
                return

            if step == "user_message":
                msg = str((form or {}).get("msg", "")).strip() or pi_runner.RESUME_MESSAGE
                text, _, reason = self._run_critic_hop(
                    task_id, msg, tools="bash,read,edit,write,mcp", log_suffix="user"
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                questions = parse_questions(text)
                state = paths.read_plan_review_state(task_id)
                if questions:
                    rnd = int(state.get("round", 1)) + 1
                    state["round"] = rnd
                    state.setdefault("rounds", []).append(
                        {"questions": questions, "answers": {}}
                    )
                    state["phase"] = "awaiting_answers"
                else:
                    state["phase"] = "awaiting_approval"
                paths.write_plan_review_state(task_id, state)
                return

        except Exception as e:
            logger.exception("plan_review step %s failed for %s", step, task_id)
            state = paths.read_plan_review_state(task_id)
            state["phase"] = "error"
            state["error"] = str(e)
            state["error_detail"] = getattr(e, "detail", "")
            state["error_step"] = step
            state["finished_at"] = time.time()
            paths.write_plan_review_state(task_id, state)

    def retry_from_error(self, task_id: str) -> None:
        """Resume the failed review step, continuing the critic's pi session."""
        state = paths.read_plan_review_state(task_id)
        if state.get("phase") != "error":
            return
        step = state.get("error_step") or "critique"
        running_phase = {
            "critique": "review_running",
            "followup": "resuming",
            "grill": "resuming",
            "revise": "revising",
            "reject": "revising",
        }.get(step, "review_running")
        state["phase"] = running_phase
        state["error"] = ""
        state["error_detail"] = ""
        paths.write_plan_review_state(task_id, state)
        self.enqueue_step(task_id, step)

    # -- rendering --------------------------------------------------------------

    def _read_todo(self, task_id: str) -> str:
        try:
            return paths.read_plan_artefact(task_id, "todo.md")
        except FileNotFoundError:
            return ""

    def render_status(self, task_id: str, oob: bool = False) -> str:
        safe_id = _esc(task_id)
        state = paths.read_plan_review_state(task_id)
        phase = state.get("phase", "idle")
        content_id = self.stage_content_id()
        target = f"#{content_id}"
        parts: list[str] = []

        turns_html = render_turns_trajectory(state.get("turns", []))
        qa_html = render_qa_history(state.get("rounds", []))
        plan_md = state.get("plan_md", "")
        if not plan_md and phase not in ("idle",):
            try:
                plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
            except FileNotFoundError:
                plan_md = ""
        todo_md = self._read_todo(task_id)

        # Trajectory first in every phase; the spinner / error / forms follow it.
        if phase != "done":
            parts.append(turns_html)

        if phase == "awaiting_answers":
            parts.append(qa_html)
            rnd = int(state.get("round", 1))
            questions = state.get("rounds", [{}])[-1].get("questions", [])
            parts.append(
                render_questions_form(
                    self.action_url(task_id, "answers"),
                    target,
                    rnd,
                    None,
                    questions,
                    meta=f"Round {rnd} — the plan critic needs clarification:",
                )
            )
        elif phase == "awaiting_approval":
            parts.append(qa_html)
            if plan_md:
                parts.append(
                    f'<div class="stage-msg plan-final"><h3>Revised plan</h3>'
                    f"{_ui._render_markdown(plan_md)}</div>"
                )
            if todo_md:
                parts.append(
                    f'<div class="stage-msg plan-todo"><h3>Implementation checklist</h3>'
                    f"{_ui._render_markdown(todo_md)}</div>"
                )
            parts.append(f"""
            <div class="stage-msg approval-controls">
              <form hx-post="{self.action_url(task_id, "approve")}"
                    hx-target="#{content_id}" hx-swap="outerHTML" style="display:inline">
                <button type="submit" class="primary">Approve</button>
              </form>
              <form hx-post="{self.action_url(task_id, "reject")}"
                    hx-target="#{content_id}" hx-swap="outerHTML" class="reject-form">
                <label>Reject with reason
                  <textarea name="reason" rows="2" required placeholder="What needs to change?"></textarea>
                </label>
                <button type="submit" class="secondary">Reject</button>
              </form>
              <form hx-post="{self.action_url(task_id, "grill")}"
                    hx-target="#{content_id}" hx-swap="outerHTML" class="grill-form">
                <label>Grill more on topic
                  <textarea name="topic" rows="2" required placeholder="Topic to drill into…"></textarea>
                </label>
                <button type="submit" class="secondary">Grill more</button>
              </form>
            </div>
            """)
        elif phase == "done":
            parts.append('<div class="stage-msg"><p class="success">Approved ✓</p></div>')
            parts.append(qa_html)
            if plan_md:
                parts.append(
                    f'<div class="stage-msg plan-final">{_ui._render_markdown(plan_md)}</div>'
                )
            if todo_md:
                parts.append(
                    f'<div class="stage-msg plan-todo">{_ui._render_markdown(todo_md)}</div>'
                )

        if phase == "error":
            parts.append(
                render_error_msg(state.get("error", ""), state.get("error_detail", ""))
            )

        if phase in ("idle", "done", "error", "stopped"):
            label = "Re-review" if phase in ("done", "error", "stopped") else "Start review"
            if self.is_enabled(task_id) or phase in ("done", "error", "stopped"):
                buttons = (
                    f'<button hx-post="{self.start_url(task_id)}" '
                    f'hx-target="#{content_id}" hx-swap="outerHTML">{label}</button>'
                )
                if phase == "error":
                    buttons += render_retry_button(self, task_id, state.get("error_step"))
                parts.append(f'<div class="stage-msg stage-buttons">{buttons}</div>')

        if phase in ("error", "stopped"):
            parts.append(render_message_form(self, task_id))

        if phase in RUNNING_PHASES:
            label = {
                "review_running": "Reviewing plan…",
                "resuming": "Processing answers…",
                "revising": "Revising plan…",
            }.get(phase, "Working…")
            parts.append(render_spinner(label))
            parts.append(render_stop_button(self, task_id))

        msg_count = len(state.get("turns", [])) + len(
            [r for r in state.get("rounds", []) if r.get("answers")]
        )
        if phase == "awaiting_answers":
            msg_count += 1
        if phase in ("awaiting_approval", "done"):
            msg_count += 2
        msg_count = max(msg_count, 1)

        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=phase in RUNNING_PHASES,
            extra_class="plan-review-content",
            oob=oob,
        )


STAGE = S03PlanReview()
