"""Stage s02: Plan — agent-driven clarifying Q&A, then a structured final plan.

Step-driven state machine persisted to tasks/<id>/plan.json (no long-lived
blocking threads): each "Submit answers" POST kicks the next background step.

Phases: draft_running → awaiting_answers → resuming → (more rounds) →
final_running → done | error. Rounds are agent-driven, hard-capped at
MAX_ROUNDS: after each answered round the draft agent emits either ≤5 more
questions (a fenced ```questions JSON block) or the READY_TO_PLAN sentinel.
"""

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
    render_qa_history,
    render_questions_form,
    render_retry_button,
    render_spinner,
    render_turns_trajectory,
)
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

MAX_ROUNDS = 3
READY_SENTINEL = "READY_TO_PLAN"
READ_ONLY_REMINDER = (
    "Remember: DO NOT write or edit any files yet. "
    "This is a read-only exploration and planning phase."
)
DRAFT_TURNS_PER_STEP = 10
DRAFT_MAX_HOPS_PER_STEP = 3
DRAFT_MAX_TURNS = 30
DRAFT_TIMEOUT_SECONDS = 300

RUNNING_PHASES = ("draft_running", "resuming", "final_running")
_QUESTIONS_BLOCK_RE = re.compile(r"```questions\s*\n(.*?)```", re.DOTALL)


def _esc(text: str) -> str:
    return _ui._esc(text)


def _strip_control_blocks(text: str) -> str:
    """Remove questions blocks and the READY_TO_PLAN sentinel."""
    text = _QUESTIONS_BLOCK_RE.sub("", text)
    text = text.replace(READY_SENTINEL, "")
    return text.strip()


class S02Plan(Stage):
    slug = "plan"
    name = "Plan"
    parts = [
        Part("draft", "Draft agent (Q&A rounds)", "draft.md", ULTRA_MODEL),
        Part("final", "Final plan (single-shot)", "final_plan.md", ULTRA_MODEL),
    ]

    def status(self, task_id: str) -> str:
        phase = paths.read_plan_state(task_id).get("phase", "idle")
        if phase in ("draft_running", "resuming", "final_running"):
            return "running"
        if phase == "awaiting_answers":
            return "awaiting"
        if phase == "done":
            return "done"
        if phase == "error":
            return "error"
        return "idle"

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        explore = stages.get("explore")
        return explore is not None and explore.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        """(Re)start the plan draft. Idempotent while any phase is running."""
        state = paths.read_plan_state(task_id)
        if state.get("phase") in RUNNING_PHASES:
            return

        content = paths.read_all_prompts_joined(task_id)
        if not content:
            paths.write_plan_state(
                task_id, {"phase": "error", "error": "no prompt files to plan from"}
            )
            return

        explore_summary = paths.read_explore_state(task_id).get("summary_md", "")

        shutil.rmtree(paths.plan_sessions_dir(task_id), ignore_errors=True)

        paths.write_plan_state(
            task_id,
            {
                "phase": "draft_running",
                "round": 1,
                "rounds": [],
                "lay_of_the_land": "",
                "final_md": "",
                "error": "",
                "explore_summary": explore_summary,
                "started_at": time.time(),
                "finished_at": None,
            },
        )
        self.enqueue_step(task_id, "draft")

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "draft":
            self._run_draft_step(task_id, initial=True)
        elif step == "resume":
            self._run_draft_step(task_id, initial=False)
        elif step == "final":
            self._run_final(task_id)
        else:
            super().run_step(task_id, step, form)

    def handle_action(self, action: str, task_id: str, form) -> None:
        if action == "answers":
            self.submit_answers(task_id, form)
            return
        super().handle_action(action, task_id, form)

    def submit_answers(self, task_id: str, form) -> None:
        """Record the current round's answers and kick the resume step."""
        state = paths.read_plan_state(task_id)
        if state.get("phase") != "awaiting_answers" or not state.get("rounds"):
            return

        rnd = int(state.get("round", 1))
        current = state["rounds"][-1]
        current["answers"] = collect_answers(form, current.get("questions", []))
        paths.write_plan_artefact(
            task_id, f"round_{rnd}_answers.json", json.dumps(current["answers"], indent=2)
        )

        state["round"] = rnd + 1
        state["phase"] = "resuming"
        paths.write_plan_state(task_id, state)
        self.enqueue_step(task_id, "resume")

    # -- background steps -------------------------------------------------------

    def _run_draft_step(self, task_id: str, initial: bool) -> None:
        start = time.monotonic()
        try:
            state = paths.read_plan_state(task_id)
            rnd = int(state.get("round", 1))

            if initial:
                message = (
                    self.load_template("draft.md")
                    .replace("{content}", paths.read_all_prompts_joined(task_id))
                    .replace(
                        "{explore_summary}",
                        state.get("explore_summary") or "(no exploration summary available)",
                    )
                )
            else:
                qa = format_qa_for_prompt(state["rounds"][-1])
                message = self.load_template("draft_followup.md").replace("{qa}", qa)

            # Turns accumulate across draft steps (rounds); persist to plan.json
            # incrementally so a refresh restores the whole trajectory like Explore.
            existing_turns = list(state.get("turns", []))
            turns: list[dict] = []

            def persist(current_turn: dict, hop: int) -> None:
                turns.append(
                    {
                        "hop": hop,
                        "text": current_turn.get("text", ""),
                        "thinking": current_turn.get("thinking", ""),
                        "tool_blocks": list(current_turn.get("tool_blocks", [])),
                        "elapsed": current_turn.get("elapsed"),
                    }
                )
                st = paths.read_plan_state(task_id)
                st["turns"] = existing_turns + turns
                paths.write_plan_state(task_id, st)

            reason = "hop_cap"
            hop = 0
            while reason == "hop_cap" and hop < DRAFT_MAX_HOPS_PER_STEP:
                hop += 1
                reason = pi_runner.run_agent_hop(
                    log_prefix=f"plan-draft-r{rnd}",
                    model=self.model_for("draft"),
                    session_id=f"plan-{task_id}",
                    sessions_dir=paths.plan_sessions_dir(task_id),
                    tools="bash,read,mcp",
                    message=message,
                    start=start,
                    sentinel=None,
                    turns_per_hop=DRAFT_TURNS_PER_STEP,
                    max_turns=DRAFT_MAX_TURNS,
                    timeout_seconds=DRAFT_TIMEOUT_SECONDS,
                    total_turns=pi_runner.count_turn_groups(turns),
                    persist_turn=persist,
                    hop=hop,
                )
                logger.info(
                    "plan: draft step round=%d hop=%d finished reason=%s", rnd, hop, reason
                )
                if reason != "hop_cap":
                    break
                message = (
                    "Stop calling tools now. Based on what you have gathered so far, "
                    "write your Lay of the land, then emit either the ```questions "
                    f"JSON block (at most 5 questions) or "
                    f"{READY_SENTINEL} on its own line."
                )

            if reason == "empty":
                raise RuntimeError("pi returned empty responses (no content in any turn)")

            text = "\n\n".join(t["text"] for t in turns if t.get("text")).strip()
            if not text:
                raise RuntimeError("plan draft step produced no text")

            questions = parse_questions(text)
            lay = _strip_control_blocks(text)

            state = paths.read_plan_state(task_id)
            if lay:
                state["lay_of_the_land"] = lay
                paths.write_plan_artefact(task_id, "draft.md", lay)

            if READY_SENTINEL in text or rnd >= MAX_ROUNDS or not questions:
                if not questions and READY_SENTINEL not in text and rnd < MAX_ROUNDS:
                    logger.warning(
                        "plan: no questions block and no sentinel in round %d; going to final",
                        rnd,
                    )
                state["phase"] = "final_running"
                paths.write_plan_state(task_id, state)
                self.enqueue_step(task_id, "final")
                return

            state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
            state["phase"] = "awaiting_answers"
            paths.write_plan_state(task_id, state)
            paths.write_plan_artefact(
                task_id, f"round_{rnd}_questions.json", json.dumps(questions, indent=2)
            )
            logger.info("plan: round %d produced %d questions", rnd, len(questions))
        except Exception as e:
            logger.exception("plan draft step failed for %s", task_id)
            state = paths.read_plan_state(task_id)
            state["phase"] = "error"
            state["error"] = str(e)
            state["error_detail"] = getattr(e, "detail", "")
            state["error_step"] = "draft" if initial else "resume"
            state["finished_at"] = time.time()
            paths.write_plan_state(task_id, state)

    def _run_final(self, task_id: str) -> None:
        try:
            state = paths.read_plan_state(task_id)
            qa_all = "\n\n".join(
                f"Round {i}:\n{format_qa_for_prompt(r)}"
                for i, r in enumerate(state.get("rounds", []), 1)
            )
            prompt = (
                self.load_template("final_plan.md")
                .replace("{content}", paths.read_all_prompts_joined(task_id))
                .replace(
                    "{explore_summary}",
                    state.get("explore_summary") or "(no exploration summary available)",
                )
                .replace("{lay_of_the_land}", state.get("lay_of_the_land") or "(none)")
                .replace("{qa}", qa_all or "(no clarifying Q&A — the draft agent had enough)")
            )
            final_md, final_elapsed = pi_runner.run_pi_text(
                prompt,
                log_prefix="plan-final",
                model=self.model_for("final"),
            )
            if "DO NOT write or edit any files" not in final_md:
                final_md = final_md.rstrip() + "\n\n" + READ_ONLY_REMINDER + "\n"
            paths.write_plan_artefact(task_id, "final_plan.md", final_md)

            state = paths.read_plan_state(task_id)
            state["final_md"] = final_md
            state["final_elapsed"] = final_elapsed
            state["phase"] = "done"
            state["finished_at"] = time.time()
            paths.write_plan_state(task_id, state)
            logger.info("plan: done for %s (%d chars)", task_id, len(final_md))
            git_utils.commit(paths.task_dir(task_id), f"plan complete {task_id}")

            from crack_server import stages

            review = stages.get("plan_review")
            if review is not None:
                review.regenerate_todo(task_id)
                review.start(task_id)
        except Exception as e:
            logger.exception("plan final failed for %s", task_id)
            state = paths.read_plan_state(task_id)
            state["phase"] = "error"
            state["error"] = str(e)
            state["error_detail"] = getattr(e, "detail", "")
            state["error_step"] = "final"
            state["finished_at"] = time.time()
            paths.write_plan_state(task_id, state)

    def retry_from_error(self, task_id: str) -> None:
        """Resume the failed draft/resume/final step, continuing the pi session."""
        state = paths.read_plan_state(task_id)
        if state.get("phase") != "error":
            return
        step = state.get("error_step") or "draft"
        running_phase = {
            "draft": "draft_running",
            "resume": "resuming",
            "final": "final_running",
        }.get(step, "draft_running")
        state["phase"] = running_phase
        state["error"] = ""
        state["error_detail"] = ""
        paths.write_plan_state(task_id, state)
        self.enqueue_step(task_id, step)

    # -- rendering --------------------------------------------------------------

    def _count_msgs(self, state: dict, phase: str) -> int:
        n = len(state.get("turns", []))
        n += len([r for r in state.get("rounds", []) if r.get("answers")])
        if phase == "awaiting_answers":
            n += 1
        if phase == "done" and state.get("final_md"):
            n += 1
        return max(n, 1)

    def render_status(self, task_id: str, oob: bool = False) -> str:
        safe_id = _esc(task_id)
        state = paths.read_plan_state(task_id)
        phase = state.get("phase", "idle")
        rnd = int(state.get("round", 1))
        content_id = self.stage_content_id()
        target = f"#{content_id}"

        parts: list[str] = []
        trajectory = render_turns_trajectory(state.get("turns", []))

        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"planned {_ui._format_ago(finished_at)}" if finished_at else "planned"
            rounds = len(state.get("rounds", []))
            meta += f" · {rounds} Q&A round{'s' if rounds != 1 else ''}"
            final_elapsed = state.get("final_elapsed")
            if final_elapsed is not None:
                meta += f" · final {final_elapsed:.1f}s"
            parts.append(f'<div class="stage-msg plan-meta"><small>{meta}</small></div>')

        # Trajectory first, then any Q&A form / final plan / error / spinner below it,
        # so the spinner (and error) always sit under the last added item.
        parts.append(trajectory)

        if phase in ("awaiting_answers", "done"):
            parts.append(render_qa_history(state.get("rounds", [])))

        if phase == "awaiting_answers":
            questions = state.get("rounds", [{}])[-1].get("questions", [])
            parts.append(
                render_questions_form(
                    self.action_url(task_id, "answers"),
                    target,
                    rnd,
                    MAX_ROUNDS,
                    questions,
                    meta=f"Round {rnd}/{MAX_ROUNDS} — the planner needs clarification:",
                )
            )
        elif phase == "done":
            final_md = state.get("final_md", "")
            if final_md:
                parts.append(
                    f'<div class="stage-msg plan-final">{_ui._render_markdown(final_md)}</div>'
                )
            parts.append(
                f'<div class="stage-msg"><small style="color: #666;">On disk: '
                f"<code>tasks/{safe_id}/plan/final_plan.md</code></small></div>"
            )

        if phase == "error":
            parts.append(
                render_error_msg(state.get("error", ""), state.get("error_detail", ""))
            )

        if phase in ("idle", "done", "error"):
            label = "Re-plan" if phase in ("done", "error") else "Plan"
            buttons = (
                f'<button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">{label}</button>'
            )
            if phase == "error":
                buttons += render_retry_button(self, task_id, state.get("error_step"))
            parts.append(f'<div class="stage-msg stage-buttons">{buttons}</div>')

        if phase in RUNNING_PHASES:
            label = {
                "draft_running": "Drafting plan… round 1",
                "resuming": f"Drafting plan… round {rnd}",
                "final_running": "Writing final plan…",
            }.get(phase, "Working…")
            parts.append(render_spinner(label))

        msg_count = self._count_msgs(state, phase)
        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=phase in RUNNING_PHASES,
            extra_class="plan-content",
            oob=oob,
        )


STAGE = S02Plan()
