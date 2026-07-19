"""Planner persona: grill → human Q&A → write report."""

from __future__ import annotations

import json
import logging
import time
from pathlib import Path

from crack_server import paths
from crack_server.stages.qa import (
    collect_answers,
    format_qa_for_prompt,
    parse_questions,
)
from crack_server.sub_agents.base import SubAgentPersona, TERMINAL_PHASES

logger = logging.getLogger("uvicorn.error")

MAX_ROUNDS = 3
READY_SENTINEL = "READY_TO_PLAN"


class PlannerPersona(SubAgentPersona):
    slug = "planner"
    name = "Planner"
    report_instructions = (
        "A structured implementation plan with problem statement, proposed changes, "
        "verification steps, and explicit risks/trade-offs debated with the human."
    )
    templates = ["system.md", "nudge.md", "grill.md", "followup.md", "write.md"]

    def run_step(
        self, run_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        if step == "run_start":
            return self._run_grill(run_id, initial=True)
        if step == "grill":
            return self._run_grill(run_id, initial=False)
        if step == "followup":
            return self._run_followup(run_id)
        if step == "write":
            return self._run_write(run_id)
        return super().run_step(run_id, step, form)

    def _qa_blob(self, run_id: str) -> str:
        state = self.state_read(run_id)
        parts = []
        for rnd in state.get("rounds") or []:
            parts.append(format_qa_for_prompt(rnd))
        return "\n\n".join(p for p in parts if p.strip())

    def _run_grill(self, run_id: str, *, initial: bool) -> tuple[str, dict | None] | None:
        state = self.state_read(run_id)
        if state.get("phase") in TERMINAL_PHASES:
            return None

        def _phase(s: dict) -> dict:
            s["phase"] = "running"
            if initial:
                s["round"] = 1
                s.setdefault("rounds", [])
            return s

        self.state_update(run_id, _phase)

        template = "grill.md"
        message = (
            self.load_template(template)
            .replace("{instructions}", state.get("instructions", ""))
            .replace("{report_path}", state.get("report_path", ""))
            .replace("{report_instructions}", self.report_instructions)
            .replace("{qa}", self._qa_blob(run_id))
        )
        return self._run_named_hop(run_id, message, template, step_label="grill")

    def _run_followup(self, run_id: str) -> tuple[str, dict | None] | None:
        state = self.state_read(run_id)
        template = "followup.md"
        message = (
            self.load_template(template)
            .replace("{instructions}", state.get("instructions", ""))
            .replace("{report_path}", state.get("report_path", ""))
            .replace("{report_instructions}", self.report_instructions)
            .replace("{qa}", self._qa_blob(run_id))
        )

        def _resuming(s: dict) -> dict:
            s["phase"] = "resuming"
            return s

        self.state_update(run_id, _resuming)
        return self._run_named_hop(run_id, message, template, step_label="followup")

    def _run_write(self, run_id: str) -> tuple[str, dict | None] | None:
        state = self.state_read(run_id)

        def _writing(s: dict) -> dict:
            s["phase"] = "writing"
            return s

        self.state_update(run_id, _writing)
        template = "write.md"
        message = (
            self.load_template(template)
            .replace("{instructions}", state.get("instructions", ""))
            .replace("{report_path}", state.get("report_path", ""))
            .replace("{report_instructions}", self.report_instructions)
            .replace("{qa}", self._qa_blob(run_id))
        )
        return self._run_named_hop(run_id, message, template, step_label="write")

    def _run_named_hop(
        self, run_id: str, message: str, template: str, *, step_label: str
    ) -> tuple[str, dict | None] | None:
        """One pi hop with a fixed message; post-process for planner control flow."""
        from crack_server.stages.steprun import TurnPersister, prompt_recorder
        from crack_server import pi_runner
        from crack_server.sub_agents import SUBAGENT_TIMEOUT_SECONDS

        state = self.state_read(run_id)
        if state.get("stop_requested"):
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None

        chat_id = state["chat_id"]
        hop_n = int(state.get("hops_completed", 0)) + 1
        state_obj = self.state(run_id)
        persister = TurnPersister(state_obj, key="turns")
        record = prompt_recorder(persister, f"{step_label} hop {hop_n}", template, message)

        try:
            reason = pi_runner.run_agent_hop(
                log_prefix=f"sub_agent/{self.slug}/{run_id}",
                model=self.model_for(),
                session_id=f"subagent-{run_id}",
                sessions_dir=paths.run_sessions_dir(chat_id, run_id),
                tools=None,
                message=message,
                start=time.monotonic(),
                sentinel=None,
                timeout_seconds=SUBAGENT_TIMEOUT_SECONDS,
                persist_turn=persister.persist,
                hop=hop_n,
                pid_file=paths.run_pid_file(chat_id, run_id),
                stop_check=lambda: bool(self.state_read(run_id).get("stop_requested")),
                record_prompt=record,
                env_extra=self._subagent_env(state),
            )
        except pi_runner.PiStopped:
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None
        except Exception as exc:
            from crack_server.sub_agents import runner

            def _fail(s: dict) -> dict:
                s["phase"] = "error"
                s["error"] = str(exc)
                s["error_detail"] = getattr(exc, "detail", "")
                s["error_step"] = step_label
                s["finished_at"] = time.time()
                return s

            self.state_update(run_id, _fail)
            runner.finish(run_id, "error")
            return None

        def _bump(s: dict) -> dict:
            s["hops_completed"] = int(s.get("hops_completed", 0)) + 1
            return s

        self.state_update(run_id, _bump)

        if reason == "stopped":
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None

        if step_label == "write":
            return self._after_hop(run_id, reason, persister)

        text = persister.text()
        questions = parse_questions(text)
        if questions:
            return self._await_answers(run_id, questions)
        if READY_SENTINEL in text or not questions:
            round_num = int(self.state_read(run_id).get("round", 1))
            if round_num >= MAX_ROUNDS:
                return ("write", {"run_id": run_id, "started_token": self.state_read(run_id).get("started_token")})
            # No questions and not ready — nudge once via write if we've debated enough
            return ("write", {"run_id": run_id, "started_token": self.state_read(run_id).get("started_token")})
        return ("write", {"run_id": run_id, "started_token": self.state_read(run_id).get("started_token")})

    def _await_answers(self, run_id: str, questions: list[dict]) -> None:
        state = self.state_read(run_id)
        chat_id = state["chat_id"]
        round_num = int(state.get("round", 1))
        run_directory = paths.run_dir(chat_id, run_id)
        questions_path = run_directory / f"round_{round_num}_questions.json"
        questions_path.write_text(json.dumps(questions, indent=2), encoding="utf-8")

        def _wait(s: dict) -> dict:
            s["phase"] = "awaiting_answers"
            s["pending_questions"] = questions
            s["round"] = round_num
            return s

        self.state_update(run_id, _wait)
        return None

    def submit_answers(self, run_id: str, form) -> None:
        state = self.state_read(run_id)
        if state.get("phase") != "awaiting_answers":
            return
        questions = state.get("pending_questions") or []
        answers = collect_answers(form, questions)
        chat_id = state["chat_id"]
        round_num = int(state.get("round", 1))
        run_directory = paths.run_dir(chat_id, run_id)
        (run_directory / f"round_{round_num}_answers.json").write_text(
            json.dumps(answers, indent=2), encoding="utf-8"
        )

        def _record(s: dict) -> dict:
            rounds = list(s.get("rounds") or [])
            rounds.append({"questions": questions, "answers": answers})
            s["rounds"] = rounds
            s.pop("pending_questions", None)
            s["round"] = round_num + 1
            s["phase"] = "resuming"
            return s

        updated = self.state_update(run_id, _record)
        self.enqueue_step(
            run_id,
            "followup",
            {"run_id": run_id, "started_token": updated.get("started_token")},
        )

    def continue_to_write(self, run_id: str) -> None:
        state = self.state_read(run_id)
        if state.get("phase") not in ("awaiting_answers", "running", "resuming"):
            return

        def _ready(s: dict) -> dict:
            s["phase"] = "writing"
            s.pop("pending_questions", None)
            return s

        updated = self.state_update(run_id, _ready)
        self.enqueue_step(
            run_id,
            "write",
            {"run_id": run_id, "started_token": updated.get("started_token")},
        )
