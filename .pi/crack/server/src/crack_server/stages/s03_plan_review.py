"""Stage s03: Plan Review — glm critic grills the user, edits final_plan.md in place,
regenerates todo.md, and gates on Approve / Reject / Grill-more."""

from __future__ import annotations

import json
import logging
import shutil
import time

from crack_server import git_utils, paths, pi_runner
from crack_server.state import JsonState
from crack_server.stages.base import Part, Stage
from crack_server.stages.qa import (
    collect_answers,
    format_qa_for_prompt,
    parse_questions,
    render_qa_history,
    render_questions_form,
)
from crack_server.stages.render import (
    render_error_msg,
    render_fatal_error_banner,
    render_message_form,
    render_retry_button,
    render_running_tail,
    render_turn_msgs,
)
from crack_server import ui as _ui
from crack_server.stages.s02_plan import REQUIRED_PLAN_HEADINGS
from crack_server.stages.steprun import (
    error_recorder,
    file_content_hash,
    grant_error_budget,
    hop_with_nudge,
    prompt_recorder,
    record_errors,
    run_until_verified,
    task_prompt_media,
    turn_persister,
    verify_artifact_file,
)

logger = logging.getLogger("uvicorn.error")

GLM_MODEL = "nvidia/z-ai/glm-5.2"
ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

MAX_AUTO_ROUNDS = 2
READY_TO_REVISE = "READY_TO_REVISE"
RUNNING_PHASES = ("review_running", "resuming", "revising")
CRITIC_TIMEOUT_SECONDS = 300
REVISE_TIMEOUT_SECONDS = 1800
REVISE_MAX_CORRECTIVE = 2

# Flow-control nudge (not a cap): sent once when the critic ended its turn
# without emitting either a questions block or the ready signal.
CRITIC_NUDGE = (
    "Stop calling tools now. Complete your response — emit either a "
    f"```questions JSON block or {READY_TO_REVISE} on its own line."
)

# Corrective message sent when a revise/reject step settled but final_plan.md
# failed on-disk verification (max REVISE_MAX_CORRECTIVE times).
REVISE_CORRECTIVE = (
    "Verification failed: {deficiency}.\n"
    "Apply the agreed changes to the plan file at {plan_path} now — use the "
    "edit tool to modify it (or write to rewrite it), preserving its required "
    "structure. When the file is updated, reply with a short summary and make "
    "no further tool calls."
)



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
        phase = paths.plan_review_state(task_id).read().get("phase", "idle")
        if phase in RUNNING_PHASES:
            return "running"
        if phase in ("awaiting_answers", "awaiting_approval"):
            return "awaiting"
        if phase in ("done", "error", "stopped"):
            return phase
        return "idle"

    def state(self, task_id: str) -> JsonState:
        return paths.plan_review_state(task_id)

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        plan = stages.get("plan")
        return plan is not None and plan.status(task_id) == "done"

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        review = paths.plan_review_state(task_id)
        if review.read().get("phase") in RUNNING_PHASES:
            return

        try:
            plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            review.write({"phase": "error", "error": "no final_plan.md — run Plan first"})
            return

        shutil.rmtree(paths.plan_review_sessions_dir(task_id), ignore_errors=True)

        fresh = {
            "phase": "review_running",
            "round": 1,
            "rounds": [],
            "turns": [],
            "errors": [],
            "error_budget": pi_runner.MAX_TOTAL_ERRORS,
            "plan_md": plan_md,
            "iterations": 0,
            "error": "",
            "started_at": time.time(),
            "finished_at": None,
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        review.write(fresh)
        self.enqueue_step(task_id, "critique", form)

    def run_step(
        self, task_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        if step in ("critique", "followup", "grill", "revise", "reject", "user_message"):
            return self._run_review_step(task_id, step, form)
        return super().run_step(task_id, step, form)

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
        """Tool-less single-shot: rewrite plan/todo.md from current final_plan.md.
        STOP-killable (RC5): the pi pid is published to the stage's pid file and
        an intentional kill surfaces as PiStopped, not an error."""
        try:
            plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            logger.warning("plan_review: no final_plan.md for todo regen on %s", task_id)
            return
        prompt = self.load_template("todo.md").replace("{plan}", plan)

        def record(entry: dict) -> None:
            entry["label"] = "todo"
            entry["template"] = "todo.md"

            def _append(state: dict) -> dict:
                state.setdefault("turns", []).append(entry)
                return state

            paths.plan_review_state(task_id).update(_append)

        todo_md, _ = pi_runner.run_pi_text(
            prompt,
            log_prefix="plan-review-todo",
            model=self.model_for("todo"),
            record_prompt=record,
            record_error=error_recorder(paths.plan_review_state(task_id)),
            pid_file=paths.stage_pid_file(task_id, self.slug),
            stop_check=lambda: bool(self.state_read(task_id).get("stop_requested")),
        )
        paths.write_plan_artefact(task_id, "todo.md", todo_md)
        logger.info("plan_review: regenerated todo.md for %s (%d chars)", task_id, len(todo_md))

    # -- action handlers ------------------------------------------------------

    def _submit_answers(self, task_id: str, form) -> None:
        review = paths.plan_review_state(task_id)
        current = review.read()
        if current.get("phase") != "awaiting_answers" or not current.get("rounds"):
            return
        rnd = int(current.get("round", 1))
        answers = collect_answers(form, current["rounds"][-1].get("questions", []))
        paths.write_plan_artefact(
            task_id, f"review_round_{rnd}_answers.json",
            json.dumps(answers, indent=2),
        )

        recorded = False

        def _record(state: dict) -> dict:
            nonlocal recorded
            if state.get("phase") != "awaiting_answers" or not state.get("rounds"):
                return state
            recorded = True
            state["rounds"][-1]["answers"] = answers
            state["phase"] = "resuming"
            return state

        review.update(_record)
        if recorded:
            self.enqueue_step(task_id, "followup")

    def _approve(self, task_id: str) -> None:
        approved = False

        def _do(state: dict) -> dict:
            nonlocal approved
            if state.get("phase") != "awaiting_approval":
                return state
            approved = True
            state["phase"] = "done"
            state["finished_at"] = time.time()
            return state

        paths.plan_review_state(task_id).update(_do)
        if not approved:
            return
        logger.info("plan_review: approved for %s", task_id)

        git_utils.commit(paths.task_dir(task_id), f"plan approved {task_id}")
        from crack_server import stages

        impl = stages.get("implementation")
        if impl is not None:
            impl.start(task_id)

    def _reject(self, task_id: str, reason: str) -> None:
        rejected = False

        def _do(state: dict) -> dict:
            nonlocal rejected
            if state.get("phase") != "awaiting_approval":
                return state
            rejected = True
            state["phase"] = "revising"
            state["reject_reason"] = reason
            return state

        paths.plan_review_state(task_id).update(_do)
        if rejected:
            self.enqueue_step(task_id, "reject")

    def _grill_more(self, task_id: str, topic: str) -> None:
        accepted = False

        def _do(state: dict) -> dict:
            nonlocal accepted
            if state.get("phase") != "awaiting_approval":
                return state
            accepted = True
            state["phase"] = "resuming"
            state["grill_topic"] = topic
            return state

        paths.plan_review_state(task_id).update(_do)
        if accepted:
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
        review = paths.plan_review_state(task_id)
        persister = turn_persister(review, media_dir=paths.task_dir(task_id) / "media", media_url_prefix=f"/tasks/{task_id}/media")

        def hop_once(msg: str, tmpl: str, hop: int) -> str:
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
                persist_turn=persister.persist,
                hop=hop,
                record_prompt=prompt_recorder(
                    persister, log_suffix, tmpl,
                    media=lambda: task_prompt_media(task_id),
                ),
                **self.agent_hop_kwargs(task_id),
            )

        # One flow-control nudge (not a cap) when the critic ended without a
        # questions block or the ready signal.
        text, reason = hop_with_nudge(
            run_hop=hop_once,
            message=message,
            template=template,
            nudge=CRITIC_NUDGE,
            text_so_far=persister.text,
            sentinels=(READY_TO_REVISE,),
        )
        return text, persister.new, reason

    def _run_review_step(
        self, task_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        review = paths.plan_review_state(task_id)
        with record_errors(
            review, step, log_message=f"plan_review step {step} failed for {task_id}"
        ):
            state = review.read()

            if step == "critique":
                plan_md = state.get("plan_md") or paths.read_plan_artefact(task_id, "final_plan.md")
                message = (
                    self.load_template("critique.md")
                    .replace("{content}", paths.read_all_prompts_joined(task_id))
                    .replace(
                        "{explore_summary}",
                        paths.explore_state(task_id).read().get("summary_md")
                        or "(no exploration summary)",
                    )
                    .replace("{plan}", plan_md)
                )
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp,analyze_image", log_suffix="critique",
                    template="critique.md",
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return None
                questions = parse_questions(text)
                if not questions:
                    # Questions are recommended-but-optional: a critic with
                    # nothing to ask chains straight into the revise step (the
                    # user still gates on Approve / Reject / Grill more).
                    logger.info(
                        "plan_review: critique produced no questions for %s; going to revise",
                        task_id,
                    )

                    def _to_revising_auto(state: dict) -> dict:
                        state["phase"] = "revising"
                        state["revise_auto"] = True
                        return state

                    review.update(_to_revising_auto)
                    return ("revise", None)

                def _await_first(state: dict) -> dict:
                    state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                    state["phase"] = "awaiting_answers"
                    return state

                state = review.update(_await_first)
                rnd = int(state.get("round", 1))
                paths.write_plan_artefact(
                    task_id, f"review_round_{rnd}_questions.json",
                    json.dumps(questions, indent=2),
                )
                return None

            if step == "followup":
                rnd = int(state.get("round", 1))
                qa_all = "\n\n".join(
                    f"Round {i}:\n{format_qa_for_prompt(r)}"
                    for i, r in enumerate(state.get("rounds", []), 1)
                )
                message = self.load_template("grill_followup.md").replace("{qa}", qa_all)
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp,analyze_image", log_suffix="followup",
                    template="grill_followup.md",
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return None
                questions = parse_questions(text)
                if questions and rnd < MAX_AUTO_ROUNDS and READY_TO_REVISE not in text:
                    def _await_next(state: dict) -> dict:
                        state["round"] = rnd + 1
                        state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                        state["phase"] = "awaiting_answers"
                        return state

                    review.update(_await_next)
                    paths.write_plan_artefact(
                        task_id, f"review_round_{rnd + 1}_questions.json",
                        json.dumps(questions, indent=2),
                    )
                    return None

                def _to_revising(state: dict) -> dict:
                    state["phase"] = "revising"
                    return state

                review.update(_to_revising)
                # Successor is enqueued by the worker after this job completes,
                # so it can't collide with our own in-flight job (RC1).
                return ("revise", None)

            if step == "grill":
                topic = state.get("grill_topic", "")
                message = (
                    f"The user wants to grill the plan further on this topic:\n{topic}\n\n"
                    "Emit a ```questions JSON block with at most 5 clarifying questions."
                )
                text, _, reason = self._run_critic_hop(
                    task_id, message, tools="bash,read,mcp,analyze_image", log_suffix="grill"
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

                def _await_grill(state: dict) -> dict:
                    rnd = int(state.get("round", 1)) + 1
                    state["round"] = rnd
                    state.setdefault("rounds", []).append({"questions": questions, "answers": {}})
                    state["phase"] = "awaiting_answers"
                    state.pop("grill_topic", None)
                    return state

                state = review.update(_await_grill)
                rnd = int(state.get("round", 1))
                paths.write_plan_artefact(
                    task_id, f"review_round_{rnd}_questions.json",
                    json.dumps(questions, indent=2),
                )
                return

            if step in ("revise", "reject"):
                start = time.monotonic()
                plan_path = paths.plan_dir(task_id) / "final_plan.md"
                before_hash = file_content_hash(plan_path)
                # The auto-chained revise after a no-questions critique may
                # legitimately find nothing to change; a retry after an errored
                # attempt may find the change already applied. A fresh
                # user-triggered revise/reject must actually modify the file.
                resumed = bool(state.get("revise_prompted"))
                require_change = not bool(state.get("revise_auto")) and not resumed
                if resumed:
                    # The session already holds the revise prompt: resume it
                    # instead of replaying the template.
                    message, template = pi_runner.RESUME_MESSAGE, ""
                elif step == "reject":
                    reject_reason = state.get("reject_reason", "")
                    message = (
                        self.load_template("reject.md")
                        .replace("{reason}", reject_reason)
                        .replace("{plan_path}", str(plan_path))
                    )
                    template = "reject.md"
                else:
                    message = self.load_template("revise.md").replace(
                        "{plan_path}", str(plan_path)
                    )
                    template = "revise.md"
                if not resumed:
                    def _mark_prompted(s: dict) -> dict:
                        s["revise_prompted"] = True
                        return s

                    review.update(_mark_prompted)
                persister = turn_persister(review, media_dir=paths.task_dir(task_id) / "media", media_url_prefix=f"/tasks/{task_id}/media")

                def run_hop(msg: str, hop: int) -> str:
                    tmpl = template if hop == 1 else ""
                    return pi_runner.run_agent_hop(
                        log_prefix=f"plan-review-{step}",
                        model=self.model_for("critic"),
                        session_id=f"plan-review-{task_id}",
                        sessions_dir=paths.plan_review_sessions_dir(task_id),
                        tools="bash,read,edit,write,mcp,analyze_image",
                        message=msg,
                        start=start,
                        sentinel=None,
                        timeout_seconds=REVISE_TIMEOUT_SECONDS,
                        persist_turn=persister.persist,
                        hop=hop,
                        record_prompt=prompt_recorder(
                            persister, step, tmpl,
                            media=lambda: task_prompt_media(task_id),
                        ),
                        **self.agent_hop_kwargs(task_id),
                    )

                outcome = run_until_verified(
                    start=start,
                    timeout_seconds=REVISE_TIMEOUT_SECONDS,
                    message=message,
                    run_hop=run_hop,
                    verify=lambda: verify_artifact_file(
                        plan_path, before_hash, REQUIRED_PLAN_HEADINGS,
                        require_change=require_change,
                    ),
                    corrective=lambda deficiency: REVISE_CORRECTIVE.format(
                        deficiency=deficiency, plan_path=plan_path
                    ),
                    max_corrective=REVISE_MAX_CORRECTIVE,
                    on_stopped=lambda: self.mark_stopped(task_id),
                )
                if outcome == "stopped":
                    return None

                plan_md = paths.read_plan_artefact(task_id, "final_plan.md")

                def _revised(state: dict) -> dict:
                    state["plan_md"] = plan_md
                    state["iterations"] = int(state.get("iterations", 0)) + 1
                    state.pop("reject_reason", None)
                    state.pop("revise_auto", None)
                    state.pop("revise_prompted", None)
                    return state

                review.update(_revised)
                try:
                    self.regenerate_todo(task_id)
                except pi_runner.PiStopped:
                    self.mark_stopped(task_id)
                    return None

                def _await_approval(state: dict) -> dict:
                    state["phase"] = "awaiting_approval"
                    return state

                review.update(_await_approval)
                return None

            if step == "user_message":
                msg = str((form or {}).get("msg", "")).strip() or pi_runner.RESUME_MESSAGE
                text, _, reason = self._run_critic_hop(
                    task_id, msg, tools="bash,read,edit,write,mcp,analyze_image", log_suffix="user"
                )
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                questions = parse_questions(text)

                def _after_message(state: dict) -> dict:
                    if questions:
                        rnd = int(state.get("round", 1)) + 1
                        state["round"] = rnd
                        state.setdefault("rounds", []).append(
                            {"questions": questions, "answers": {}}
                        )
                        state["phase"] = "awaiting_answers"
                    else:
                        state["phase"] = "awaiting_approval"
                    return state

                review.update(_after_message)
                return

    def retry_from_error(self, task_id: str) -> None:
        """Resume the failed review step, continuing the critic's pi session."""
        retry = False
        step = "critique"

        def _retry(state: dict) -> dict:
            nonlocal retry, step
            if state.get("phase") != "error":
                return state
            retry = True
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
            grant_error_budget(state)
            return state

        paths.plan_review_state(task_id).update(_retry)
        if retry:
            self.enqueue_step(task_id, step)

    # -- rendering --------------------------------------------------------------

    def _read_todo(self, task_id: str) -> str:
        try:
            return paths.read_plan_artefact(task_id, "todo.md")
        except FileNotFoundError:
            return ""

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.plan_review_state(task_id).read()
        phase = state.get("phase", "idle")
        msgs: list[str] = []

        # Trajectory stays in msgs for live append; full reload of done omits it
        # to match the previous "approved" view.
        if phase != "done":
            msgs.extend(render_turn_msgs(state.get("turns", []), errors=state.get("errors", [])))

        if phase in ("awaiting_answers", "awaiting_approval", "done"):
            msgs.extend(render_qa_history(state.get("rounds", [])))

        if phase == "done":
            msgs.append('<div class="stage-msg"><p class="success">Approved ✓</p></div>')
            plan_md = state.get("plan_md", "")
            if not plan_md:
                try:
                    plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
                except FileNotFoundError:
                    plan_md = ""
            todo_md = self._read_todo(task_id)
            if plan_md:
                msgs.append(
                    f'<div class="stage-msg plan-final">{_ui._render_markdown(plan_md)}</div>'
                )
            if todo_md:
                msgs.append(
                    f'<div class="stage-msg plan-todo">{_ui._render_markdown(todo_md)}</div>'
                )

        return msgs

    def render_tail(self, task_id: str) -> str:
        state = paths.plan_review_state(task_id).read()
        phase = state.get("phase", "idle")
        content_id = self.stage_content_id()
        target = f"#{content_id}"
        parts: list[str] = []

        plan_md = state.get("plan_md", "")
        if not plan_md and phase not in ("idle", "done"):
            try:
                plan_md = paths.read_plan_artefact(task_id, "final_plan.md")
            except FileNotFoundError:
                plan_md = ""
        todo_md = self._read_todo(task_id)

        if phase == "awaiting_answers":
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
            if plan_md:
                parts.append(
                    f'<div class="plan-final"><h3>Revised plan</h3>'
                    f"{_ui._render_markdown(plan_md)}</div>"
                )
            if todo_md:
                parts.append(
                    f'<div class="plan-todo"><h3>Implementation checklist</h3>'
                    f"{_ui._render_markdown(todo_md)}</div>"
                )
            parts.append(f"""
            <div class="approval-controls">
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

        if phase == "error":
            parts.append(render_fatal_error_banner(state))
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
                parts.append(f'<div class="stage-buttons">{buttons}</div>')

        if phase in ("error", "stopped"):
            parts.append(render_message_form(self, task_id))

        if phase in RUNNING_PHASES:
            label = {
                "review_running": "Reviewing plan…",
                "resuming": "Processing answers…",
                "revising": "Revising plan…",
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
            extra_class="plan-review-content",
            oob=oob,
        )


STAGE = S03PlanReview()
