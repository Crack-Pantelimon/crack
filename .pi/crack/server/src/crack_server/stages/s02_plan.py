"""Stage s02: Plan — agent-driven clarifying Q&A, then an agent-written plan file.

Step-driven state machine persisted to tasks/<id>/plan.json (no long-lived
blocking threads): each "Submit answers" POST kicks the next background step.

Phases: draft_running → awaiting_answers → resuming → (more rounds) →
write_running → done | error. Rounds are agent-driven, hard-capped at
MAX_ROUNDS: after each answered round the draft agent emits either ≤5 more
questions (a fenced ```questions JSON block) or the READY_TO_PLAN signal.
The write step then continues the same pi session with write/edit tools and
writes plan/final_plan.md itself; completion is *verified* on disk (file
exists, changed this step, required headings present — see
steprun.run_until_verified), never declared by model text.
"""

from __future__ import annotations

import json
import logging
import shutil
import time

from crack_server import git_utils, paths, pi_runner
from crack_server.state import JsonState
from crack_server.stages.base import Part, Stage
from crack_server.stages.qa import (
    _QUESTIONS_BLOCK_RE,
    collect_answers,
    format_qa_for_prompt,
    parse_questions,
    render_qa_history,
    render_questions_form,
)
from crack_server.stages.render import (
    render_error_msg,
    render_message_form,
    render_retry_button,
    render_running_tail,
    render_turn_msgs,
)
from crack_server import ui as _ui
from crack_server.ui import _esc
from crack_server.stages.steprun import (
    file_content_hash,
    hop_with_nudge,
    prompt_recorder,
    record_errors,
    run_until_verified,
    turn_persister,
    verify_artifact_file,
)

logger = logging.getLogger("uvicorn.error")

ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

MAX_ROUNDS = 3
READY_SENTINEL = "READY_TO_PLAN"
DRAFT_TIMEOUT_SECONDS = 300
WRITE_TIMEOUT_SECONDS = 1800
WRITE_MAX_CORRECTIVE = 2

# Flow-control nudge (not a cap): sent once when the agent ended its turn
# without emitting either a questions block or the sentinel.
DRAFT_NUDGE = (
    "Stop calling tools now. Based on what you have gathered so far, "
    "write your Draft plan, then emit either the ```questions "
    "JSON block (at most 5 questions) or "
    f"{READY_SENTINEL} on its own line."
)

# Headings the write step's on-disk verification requires in final_plan.md
# (prefix-matched at line start, so e.g. "## Overview / Summary" satisfies
# "## Overview"). Must mirror the structure mandated by write_plan.md.
REQUIRED_PLAN_HEADINGS = (
    "# Plan",
    "## Initial build/check instructions",
    "## Problem statement",
    "## Changes",
    "## What NOT to change",
    "## Automatic verification",
    "## Manual verification",
    "## Overview",
)

# Corrective message sent when the write step settled but the plan file failed
# verification; names the exact deficiency (max WRITE_MAX_CORRECTIVE times).
WRITE_CORRECTIVE = (
    "Verification failed: {deficiency}.\n"
    "Write the complete implementation plan to the file at {plan_path} now — "
    "use the write tool to create/overwrite it or the edit tool to fix it, "
    "following the required structure. When the file is complete, reply with "
    "a short summary and make no further tool calls."
)

RUNNING_PHASES = ("draft_running", "resuming", "write_running")


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
        Part("write", "Plan writer (agentic)", "write_plan.md", ULTRA_MODEL),
    ]

    phase_key = "phase"
    message_phase = "resuming"

    def status(self, task_id: str) -> str:
        phase = paths.plan_state(task_id).read().get("phase", "idle")
        if phase in RUNNING_PHASES:
            return "running"
        if phase == "awaiting_answers":
            return "awaiting"
        if phase in ("done", "error", "stopped"):
            return phase
        return "idle"

    def state(self, task_id: str) -> JsonState:
        return paths.plan_state(task_id)

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        explore = stages.get("explore")
        return explore is not None and explore.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        """(Re)start the plan draft. Idempotent while any phase is running."""
        plan = paths.plan_state(task_id)
        if plan.read().get("phase") in RUNNING_PHASES:
            return

        content = paths.read_all_prompts_joined(task_id)
        if not content:
            plan.write({"phase": "error", "error": "no prompt files to plan from"})
            return

        explore_summary = paths.explore_state(task_id).read().get("summary_md", "")

        shutil.rmtree(paths.plan_sessions_dir(task_id), ignore_errors=True)
        self._clear_stale_artefacts(task_id)

        fresh = {
            "phase": "draft_running",
            "round": 1,
            "rounds": [],
            "draft_plan": "",
            "final_md": "",
            "error": "",
            "explore_summary": explore_summary,
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        plan.write(fresh)
        self.enqueue_step(task_id, "draft", form)

    def _clear_stale_artefacts(self, task_id: str) -> None:
        """Delete a previous run's plan artefacts so the write step's freshness
        check can never be satisfied by an old file."""
        plan_dir = paths.plan_dir(task_id)
        if not plan_dir.is_dir():
            return
        stale = ["draft.md", "final_plan.md", "todo.md"]
        stale += [p.name for p in plan_dir.glob("round_*_questions.json")]
        stale += [p.name for p in plan_dir.glob("round_*_answers.json")]
        for name in stale:
            try:
                (plan_dir / name).unlink(missing_ok=True)
            except OSError as e:
                logger.warning("plan: could not clear stale artefact %s: %s", name, e)

    def run_step(
        self, task_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        if step == "draft":
            return self._run_draft_step(task_id, initial=True)
        if step == "resume":
            return self._run_draft_step(task_id, initial=False)
        if step == "user_message":
            msg = str((form or {}).get("msg", "")).strip()
            return self._run_draft_step(
                task_id, initial=False, message_override=msg or pi_runner.RESUME_MESSAGE
            )
        if step == "write":
            return self._run_write_step(task_id)
        return super().run_step(task_id, step, form)

    def handle_action(self, action: str, task_id: str, form) -> None:
        if action == "answers":
            self.submit_answers(task_id, form)
            return
        super().handle_action(action, task_id, form)

    def submit_answers(self, task_id: str, form) -> None:
        """Record the current round's answers and kick the resume step."""
        plan = paths.plan_state(task_id)
        current = plan.read()
        if current.get("phase") != "awaiting_answers" or not current.get("rounds"):
            return

        rnd = int(current.get("round", 1))
        answers = collect_answers(form, current["rounds"][-1].get("questions", []))
        paths.write_plan_artefact(
            task_id, f"round_{rnd}_answers.json", json.dumps(answers, indent=2)
        )

        recorded = False

        def _record(state: dict) -> dict:
            nonlocal recorded
            if state.get("phase") != "awaiting_answers" or not state.get("rounds"):
                return state
            recorded = True
            state["rounds"][-1]["answers"] = answers
            state["round"] = rnd + 1
            state["phase"] = "resuming"
            return state

        plan.update(_record)
        if recorded:
            self.enqueue_step(task_id, "resume")

    # -- background steps -------------------------------------------------------

    def _run_draft_step(
        self, task_id: str, initial: bool, message_override: str | None = None
    ) -> tuple[str, dict | None] | None:
        start = time.monotonic()
        plan = paths.plan_state(task_id)
        step_name = "draft" if initial else "resume"
        with record_errors(plan, step_name, log_message=f"plan draft step failed for {task_id}"):
            state = plan.read()
            rnd = int(state.get("round", 1))

            if message_override is not None:
                message, template = message_override, ""
            elif initial and state.get("turns"):
                # Requeued "draft" job (B5): the pi session already holds the
                # earlier turns — resume it instead of replaying the template.
                message, template = pi_runner.RESUME_MESSAGE, ""
            elif initial:
                message = (
                    self.load_template("draft.md")
                    .replace("{content}", paths.read_all_prompts_joined(task_id))
                    .replace(
                        "{explore_summary}",
                        state.get("explore_summary") or "(no exploration summary available)",
                    )
                )
                template = "draft.md"
            elif not state.get("rounds"):
                # A resume without any answered round (e.g. retry after a
                # user_message error): nothing to compile — just continue.
                message, template = pi_runner.RESUME_MESSAGE, ""
            else:
                qa = format_qa_for_prompt(state["rounds"][-1])
                message = self.load_template("draft_followup.md").replace("{qa}", qa)
                template = "draft_followup.md"

            # Turns accumulate across draft steps (rounds); persist to plan.json
            # incrementally so a refresh restores the whole trajectory like Explore.
            persister = turn_persister(plan)

            def hop_once(msg: str, tmpl: str, hop: int) -> str:
                return pi_runner.run_agent_hop(
                    log_prefix=f"plan-draft-r{rnd}",
                    model=self.model_for("draft"),
                    session_id=f"plan-{task_id}",
                    sessions_dir=paths.plan_sessions_dir(task_id),
                    tools="bash,read,mcp",
                    message=msg,
                    start=start,
                    sentinel=None,
                    timeout_seconds=DRAFT_TIMEOUT_SECONDS,
                    persist_turn=persister.persist,
                    hop=hop,
                    record_prompt=prompt_recorder(persister, f"round {rnd}", tmpl),
                    **self.agent_hop_kwargs(task_id),
                )

            # One flow-control nudge (not a cap) when the agent ended without
            # emitting either a questions block or the sentinel.
            text, reason = hop_with_nudge(
                run_hop=hop_once,
                message=message,
                template=template,
                nudge=DRAFT_NUDGE,
                text_so_far=persister.text,
                sentinels=(READY_SENTINEL,),
                on_stopped=lambda: self.mark_stopped(task_id),
            )
            logger.info("plan: draft step round=%d finished reason=%s", rnd, reason)
            if reason == "stopped":
                return None
            if not text:
                raise RuntimeError("plan draft step produced no text")

            questions = parse_questions(text)
            draft_plan = _strip_control_blocks(text)

            if draft_plan:
                paths.write_plan_artefact(task_id, "draft.md", draft_plan)

            if READY_SENTINEL in text or rnd >= MAX_ROUNDS or not questions:
                if not questions and READY_SENTINEL not in text and rnd < MAX_ROUNDS:
                    # Questions are recommended-but-optional: a model that emits
                    # neither signal still advances — the write step's on-disk
                    # verification is what makes advancing safe.
                    logger.warning(
                        "plan: no questions block and no %s in round %d; advancing to write",
                        READY_SENTINEL, rnd,
                    )

                def _to_write(state: dict) -> dict:
                    if draft_plan:
                        state["draft_plan"] = draft_plan
                    state["phase"] = "write_running"
                    return state

                plan.update(_to_write)
                # Successor is enqueued by the worker after this job completes,
                # so it can't collide with our own in-flight job (RC1).
                return ("write", None)

            def _await_answers(state: dict) -> dict:
                if draft_plan:
                    state["draft_plan"] = draft_plan
                state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                state["phase"] = "awaiting_answers"
                return state

            plan.update(_await_answers)
            paths.write_plan_artefact(
                task_id, f"round_{rnd}_questions.json", json.dumps(questions, indent=2)
            )
            logger.info("plan: round %d produced %d questions", rnd, len(questions))
        return None

    def _run_write_step(self, task_id: str) -> None:
        """Continue the draft session with write/edit tools until the agent has
        written a verified plan/final_plan.md (see run_until_verified)."""
        start = time.monotonic()
        plan = paths.plan_state(task_id)
        plan_path = paths.plan_dir(task_id) / "final_plan.md"
        with record_errors(plan, "write", log_message=f"plan write step failed for {task_id}"):
            state = plan.read()
            before_hash = file_content_hash(plan_path)
            # A retry/resume after an errored write attempt must resume the
            # session, never replay the template — and a file the errored
            # attempt already completed may pass verification unchanged.
            resumed = bool(state.get("write_prompted"))
            if resumed:
                message, template = pi_runner.RESUME_MESSAGE, ""
            else:
                qa_all = "\n\n".join(
                    f"Round {i}:\n{format_qa_for_prompt(r)}"
                    for i, r in enumerate(state.get("rounds", []), 1)
                )
                message = (
                    self.load_template("write_plan.md")
                    .replace("{plan_path}", str(plan_path))
                    .replace("{content}", paths.read_all_prompts_joined(task_id))
                    .replace(
                        "{explore_summary}",
                        state.get("explore_summary") or "(no exploration summary available)",
                    )
                    .replace("{draft_plan}", state.get("draft_plan") or "(none)")
                    .replace("{qa}", qa_all or "(no clarifying Q&A — the planner had enough)")
                )
                template = "write_plan.md"

                def _mark_prompted(s: dict) -> dict:
                    s["write_prompted"] = True
                    return s

                plan.update(_mark_prompted)

            persister = turn_persister(plan)

            def run_hop(msg: str, hop: int) -> str:
                tmpl = template if hop == 1 else ""
                return pi_runner.run_agent_hop(
                    log_prefix="plan-write",
                    model=self.model_for("write"),
                    session_id=f"plan-{task_id}",
                    sessions_dir=paths.plan_sessions_dir(task_id),
                    tools="bash,read,edit,write,mcp",
                    message=msg,
                    start=start,
                    sentinel=None,
                    timeout_seconds=WRITE_TIMEOUT_SECONDS,
                    persist_turn=persister.persist,
                    hop=hop,
                    record_prompt=prompt_recorder(persister, "write", tmpl),
                    **self.agent_hop_kwargs(task_id),
                )

            outcome = run_until_verified(
                start=start,
                timeout_seconds=WRITE_TIMEOUT_SECONDS,
                message=message,
                run_hop=run_hop,
                verify=lambda: verify_artifact_file(
                    plan_path, before_hash, REQUIRED_PLAN_HEADINGS,
                    require_change=not resumed,
                ),
                corrective=lambda deficiency: WRITE_CORRECTIVE.format(
                    deficiency=deficiency, plan_path=plan_path
                ),
                max_corrective=WRITE_MAX_CORRECTIVE,
                on_stopped=lambda: self.mark_stopped(task_id),
            )
            if outcome == "stopped":
                return

            final_md = paths.read_plan_artefact(task_id, "final_plan.md")

            def _finish(state: dict) -> dict:
                state["final_md"] = final_md
                state["phase"] = "done"
                state["finished_at"] = time.time()
                return state

            plan.update(_finish)
            logger.info("plan: done for %s (%d chars)", task_id, len(final_md))
            git_utils.commit(paths.task_dir(task_id), f"plan complete {task_id}")

            from crack_server import stages

            review = stages.get("plan_review")
            if review is not None:
                try:
                    review.regenerate_todo(task_id)
                except Exception:
                    # A failed/stopped todo regen must not error a completed
                    # plan — the review's revise flow regenerates it anyway.
                    logger.exception("plan: todo regeneration failed for %s", task_id)
                review.start(task_id)

    def retry_from_error(self, task_id: str) -> None:
        """Resume the failed draft/resume/write step, continuing the pi session."""
        retry = False
        step = "draft"

        def _retry(state: dict) -> dict:
            nonlocal retry, step
            if state.get("phase") != "error":
                return state
            retry = True
            step = state.get("error_step") or "draft"
            if step not in ("draft", "resume", "write"):
                step = "draft"  # legacy/unknown step names (e.g. old "final")
            running_phase = {
                "draft": "draft_running",
                "resume": "resuming",
                "write": "write_running",
            }[step]
            state["phase"] = running_phase
            state["error"] = ""
            state["error_detail"] = ""
            return state

        paths.plan_state(task_id).update(_retry)
        if retry:
            self.enqueue_step(task_id, step)

    # -- rendering --------------------------------------------------------------

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.plan_state(task_id).read()
        phase = state.get("phase", "idle")
        msgs: list[str] = []

        if phase == "done":
            finished_at = state.get("finished_at")
            meta = f"planned {_ui._format_ago(finished_at)}" if finished_at else "planned"
            rounds = len(state.get("rounds", []))
            meta += f" · {rounds} Q&A round{'s' if rounds != 1 else ''}"
            msgs.append(f'<div class="stage-msg plan-meta"><small>{meta}</small></div>')

        msgs.extend(render_turn_msgs(state.get("turns", [])))

        if phase in ("awaiting_answers", "done"):
            msgs.extend(render_qa_history(state.get("rounds", [])))

        if phase == "done":
            final_md = state.get("final_md", "")
            if final_md:
                msgs.append(
                    f'<div class="stage-msg plan-final">{_ui._render_markdown(final_md)}</div>'
                )
            safe_id = _esc(task_id)
            msgs.append(
                f'<div class="stage-msg"><small style="color: #666;">On disk: '
                f"<code>tasks/{safe_id}/plan/final_plan.md</code></small></div>"
            )

        return msgs

    def render_tail(self, task_id: str) -> str:
        state = paths.plan_state(task_id).read()
        phase = state.get("phase", "idle")
        rnd = int(state.get("round", 1))
        content_id = self.stage_content_id()
        target = f"#{content_id}"
        parts: list[str] = []

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

        if phase == "error":
            parts.append(
                render_error_msg(state.get("error", ""), state.get("error_detail", ""))
            )

        if phase in ("idle", "done", "error", "stopped"):
            label = "Re-plan" if phase in ("done", "error", "stopped") else "Plan"
            buttons = (
                f'<button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">{label}</button>'
            )
            if phase == "error":
                buttons += render_retry_button(self, task_id, state.get("error_step"))
            parts.append(f'<div class="stage-buttons">{buttons}</div>')

        if phase in ("error", "stopped"):
            parts.append(render_message_form(self, task_id))

        if phase in RUNNING_PHASES:
            label = {
                "draft_running": "Drafting plan… round 1",
                "resuming": f"Drafting plan… round {rnd}",
                "write_running": "Writing plan file…",
            }.get(phase, "Working…")
            parts.append(render_running_tail(self, task_id, label))

        return "".join(parts)

    def render_status(
        self, task_id: str, oob: bool = False, after: int | None = None
    ) -> str:
        return self.wrap_status(
            task_id,
            self.render_msgs(task_id),
            self.render_tail(task_id),
            after=after,
            extra_class="plan-content",
            oob=oob,
        )


STAGE = S02Plan()
