"""Sub-agent persona base class and shared run-step machinery."""

from __future__ import annotations

import logging
import time
import uuid
from pathlib import Path

from crack_server import paths, pi_runner, queue
from crack_server.ratelimit import MAX_TOTAL_ERRORS, RESUME_MESSAGE
from crack_server.state import JsonState
from crack_server.sub_agents.constants import ORPHAN_PHASE_GRACE_SECONDS, SUBAGENT_JOB_SLUG, SUBAGENT_TIMEOUT_SECONDS
from crack_server.steprun import (
    TurnPersister,
    error_recorder,
    grant_error_budget,
    prompt_recorder,
)

logger = logging.getLogger("uvicorn.error")

TERMINAL_PHASES = frozenset({"done", "error", "stopped"})
ACTIVE_PHASES = frozenset({"running", "resuming", "writing"})
MAX_NUDGES = 3
MAX_HOPS = 5

DEFAULT_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"


class SubAgentPersona:
    slug: str = ""
    name: str = ""
    report_instructions: str = ""
    templates: list[str] = []

    # -- config (persona dir config.json) -----------------------------------

    def persona_dir(self) -> Path:
        return paths.sub_agent_persona_dir(self.slug)

    def config_path(self) -> Path:
        return self.persona_dir() / "config.json"

    def config_dict(self) -> dict:
        import json

        path = self.config_path()
        if not path.is_file():
            return {}
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}
        return data if isinstance(data, dict) else {}

    def model_for(self) -> str:
        override = self.config_dict().get("model")
        return override or DEFAULT_MODEL

    def set_model(self, model_id: str) -> None:
        import json

        path = self.config_path()
        data = self.config_dict()
        data["model"] = model_id
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def load_template(self, name: str) -> str:
        path = self.persona_dir() / Path(name).name
        if not path.is_file():
            raise RuntimeError(f"missing prompt template: {path}")
        return path.read_text(encoding="utf-8")

    def tool_name(self) -> str:
        return f"spawn_{self.slug}"

    def tool_description(self) -> str:
        return str(self.config_dict().get("tool_description", f"Spawn the {self.name} sub-agent"))

    def tool_label(self) -> str:
        return str(self.config_dict().get("tool_label", self.name))

    # -- state access ---------------------------------------------------------

    def state(self, run_id: str) -> JsonState:
        return paths.run_state_by_id(run_id)

    def state_read(self, run_id: str) -> dict:
        return self.state(run_id).read()

    def state_update(self, run_id: str, fn) -> dict:
        return self.state(run_id).update(fn)

    def _run_paths(self, run_id: str) -> tuple[str, str]:
        state = self.state_read(run_id)
        return state["chat_id"], run_id

    # -- worker queue ---------------------------------------------------------

    def enqueue_step(
        self,
        run_id: str,
        step: str,
        form: dict | None = None,
        ignore_job_id: str | None = None,
    ) -> None:
        state = self.state_read(run_id)
        chat_id = state["chat_id"]
        payload = dict(form or {})
        payload.setdefault("run_id", run_id)
        if state.get("started_token"):
            payload.setdefault("started_token", state["started_token"])
        queue.enqueue_exclusive(
            chat_id,
            SUBAGENT_JOB_SLUG,
            step,
            payload,
            ignore_job_id=ignore_job_id,
            run_id=run_id,
        )

    def prepare_start_token(self, state: dict) -> dict:
        token = uuid.uuid4().hex
        state["started_token"] = token
        return {"run_id": state["run_id"], "started_token": token}

    async def dispatch_step(
        self, run_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        token = (form or {}).get("started_token")
        if token is not None:
            current = self.state_read(run_id).get("started_token")
            if current != token:
                logger.info(
                    "sub_agent %s: dropping stale job for %s (token mismatch)",
                    self.slug, run_id,
                )
                return None
        successor = await self.run_step(run_id, step, form)
        state = self.state_read(run_id)
        phase = state.get("phase", "")
        inbox = state.get("child_inbox") or []
        if phase in ACTIVE_PHASES and inbox:
            return ("drain_children", {"run_id": run_id, "started_token": state.get("started_token")})
        return successor

    async def run_step(
        self, run_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        if step == "run_start":
            return await self._begin_run(run_id)
        if step == "run":
            return await self._run_hop(run_id, form)
        if step == "drain_children":
            from crack_server.sub_agents import resume

            return await resume.drain_children(run_id, self)
        raise NotImplementedError(f"{self.slug}: no run_step handler for {step!r}")

    async def _begin_run(self, run_id: str) -> tuple[str, dict | None] | None:
        def _start(state: dict) -> dict:
            state["phase"] = "running"
            state["hops_completed"] = 0
            state["nudge_count"] = 0
            return state

        self.state_update(run_id, _start)
        return await self._run_hop(run_id, None)

    def _compile_message(self, run_id: str, form: dict | None) -> tuple[str, str]:
        state = self.state_read(run_id)
        if form and form.get("user_answer"):
            return str(form["user_answer"]), "user_answer"
        if form and form.get("child_results"):
            return str(form["child_results"]), "child_results"
        if form and form.get("resume"):
            return RESUME_MESSAGE, ""
        if form and form.get("nudge"):
            template = "nudge.md"
            text = self._fill_template(template, state)
            return text, template
        if state.get("hops_completed", 0) > 0:
            return RESUME_MESSAGE, ""
        template = "system.md"
        text = self._fill_template(template, state)
        return text, template

    def _fill_template(self, template: str, state: dict) -> str:
        text = self.load_template(template)
        return (
            text.replace("{instructions}", state.get("instructions", ""))
            .replace("{report_path}", state.get("report_path", ""))
            .replace("{report_instructions}", self.report_instructions)
        )

    def _subagent_env(self, state: dict) -> dict[str, str]:
        return {
            "CRACK_SUBAGENT_CTX": "1",
            "CRACK_SUBAGENT_DEPTH": str(state.get("depth", 0)),
            "CRACK_CHAT_ID": state.get("chat_id", ""),
            "CRACK_PARENT_KIND": "run",
            "CRACK_PARENT_ID": state.get("run_id", ""),
        }

    async def _run_hop(
        self, run_id: str, form: dict | None
    ) -> tuple[str, dict | None] | None:
        state = self.state_read(run_id)
        if state.get("phase") in TERMINAL_PHASES:
            return None
        if state.get("stop_requested"):
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None

        chat_id = state["chat_id"]
        message, template = self._compile_message(run_id, form)
        hop_n = int(state.get("hops_completed", 0)) + 1
        state_obj = self.state(run_id)
        persister = TurnPersister(
            state_obj, key="turns",
            media_dir=paths.run_media_dir(chat_id, run_id),
            media_url_prefix=f"/chats/{chat_id}/sub_agents/runs/{run_id}/media",
        )
        record = prompt_recorder(persister, f"hop {hop_n}", template, message)

        try:
            reason = await pi_runner.arun_agent_hop(
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
                record_error=error_recorder(state_obj),
                error_budget=lambda: int(
                    state_obj.read().get("error_budget", MAX_TOTAL_ERRORS)
                ),
                env_extra=self._subagent_env(state),
                waiting_check=lambda: bool(self.state_read(run_id).get("waiting_on")),
            )
        except pi_runner.PiStopped:
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None
        except Exception as exc:
            def _fail(s: dict) -> dict:
                s["phase"] = "error"
                s["error"] = str(exc)
                s["error_detail"] = getattr(exc, "detail", "")
                s["error_over_budget"] = bool(getattr(exc, "over_budget", False))
                s["error_step"] = "run"
                s["finished_at"] = time.time()
                return s

            self.state_update(run_id, _fail)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "error")
            return None

        def _bump(state: dict) -> dict:
            state["hops_completed"] = int(state.get("hops_completed", 0)) + 1
            return state

        self.state_update(run_id, _bump)

        if reason == "stopped":
            self._mark_stopped(run_id)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return None

        return await self._after_hop(run_id, reason, persister)

    async def _after_hop(
        self, run_id: str, reason: str, persister: TurnPersister
    ) -> tuple[str, dict | None] | None:
        state = self.state_read(run_id)
        report = Path(state.get("report_path", ""))
        if report.is_file():
            def _done(s: dict) -> dict:
                s["phase"] = "done"
                s["finished_at"] = time.time()
                return s

            self.state_update(run_id, _done)
            from crack_server.sub_agents import runner

            runner.finish(run_id, "done")
            return None

        if state.get("phase") == "awaiting_user":
            # ask_user hop-termination: the run suspends until the human
            # answers (a fresh resume hop delivers it) — no nudge, no
            # successor, exactly like the planner's awaiting_answers.
            return None

        if state.get("children"):
            return None

        last_turn = persister.new[-1] if persister.new else {}
        tool_calls = last_turn.get("tool_blocks") or []
        nudge_count = int(state.get("nudge_count", 0))
        hops = int(state.get("hops_completed", 0))

        if not tool_calls and nudge_count < MAX_NUDGES:
            def _nudge(s: dict) -> dict:
                s["nudge_count"] = int(s.get("nudge_count", 0)) + 1
                return s

            self.state_update(run_id, _nudge)
            return ("run", {"nudge": True, "run_id": run_id, "started_token": state.get("started_token")})

        if tool_calls and hops < MAX_HOPS:
            return ("run", {"run_id": run_id, "started_token": state.get("started_token")})

        def _fail(s: dict) -> dict:
            s["phase"] = "error"
            s["error"] = "sub-agent finished without writing the required report"
            s.setdefault("error_detail", "")
            s["error_step"] = "run"
            s["finished_at"] = time.time()
            return s

        self.state_update(run_id, _fail)
        from crack_server.sub_agents import runner

        runner.finish(run_id, "error")
        return None

    def _mark_stopped(self, run_id: str) -> None:
        def _stop(state: dict) -> dict:
            state["phase"] = "stopped"
            state["finished_at"] = time.time()
            return state

        self.state_update(run_id, _stop)

    def check_orphaned(self, run_id: str) -> bool:
        current = self.state_read(run_id)
        if current.get("children"):
            return False
        if current.get("waiting_on"):
            # Suspended in a blocking wait_join: no job is meant to be queued.
            return False
        state_obj = self.state(run_id)
        observed = current.get("phase")
        if observed in TERMINAL_PHASES or observed in ("awaiting_answers", "awaiting_user"):
            return False
        if observed not in ACTIVE_PHASES and observed != "running":
            # running is in ACTIVE_PHASES; allow writing/resuming only
            if observed not in ("running", "resuming", "writing"):
                return False
        chat_id = state_obj.read().get("chat_id", "")
        if queue.has_job(chat_id, SUBAGENT_JOB_SLUG, run_id=run_id):
            return False
        try:
            age = time.time() - state_obj.path.stat().st_mtime
        except OSError:
            return False
        if age < ORPHAN_PHASE_GRACE_SECONDS:
            return False

        flipped = False

        def _fail(s: dict) -> dict:
            nonlocal flipped
            if s.get("phase") != observed:
                return s
            flipped = True
            s["phase"] = "error"
            s["error"] = (
                "sub-agent was in a running phase with no queued job — "
                "the job was likely dropped or lost; use Retry"
            )
            s.setdefault("error_detail", "")
            s["finished_at"] = time.time()
            return s

        state_obj.update(_fail)
        if flipped:
            logger.error(
                "sub_agent %s: orphaned phase %r for %s — marked error",
                self.slug, observed, run_id,
            )
        return flipped

    def retry(self, run_id: str) -> None:
        state = self.state_read(run_id)
        if state.get("phase") not in ("error", "stopped"):
            return

        def _reset(s: dict) -> dict:
            s["phase"] = "running"
            s["error"] = ""
            s["error_detail"] = ""
            s["error_step"] = ""
            s["stop_requested"] = False
            s["finished_at"] = None
            grant_error_budget(s)
            token = uuid.uuid4().hex
            s["started_token"] = token
            return s

        updated = self.state_update(run_id, _reset)
        self.enqueue_step(
            run_id,
            "run",
            {"run_id": run_id, "started_token": updated["started_token"], "resume": True},
        )

    def request_stop(self, run_id: str, *, cascade: bool = False) -> None:
        state = self.state_read(run_id)
        if state.get("phase") in TERMINAL_PHASES:
            return

        def _flag(s: dict) -> dict:
            s["stop_requested"] = True
            if s.get("phase") not in TERMINAL_PHASES:
                s["phase"] = "stopped"
                s["finished_at"] = time.time()
            return s

        self.state_update(run_id, _flag)
        chat_id = state["chat_id"]
        killed = pi_runner.kill_pid_file(paths.run_pid_file(chat_id, run_id))
        logger.info("sub_agent %s: stop requested for %s (killed=%s)", self.slug, run_id, killed)

        for child_id in list(state.get("children") or []):
            from crack_server.sub_agents import registry

            child = paths.run_state_by_id(child_id).read()
            persona = registry.get(child.get("persona", ""))
            if persona is not None:
                persona.request_stop(child_id, cascade=True)

        if not cascade:
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")

    def record_dispatch_error(self, run_id: str, message: str) -> None:
        def _fail(s: dict) -> dict:
            s["phase"] = "error"
            s["error"] = f"worker dispatch failed: {message}"
            s.setdefault("error_detail", "")
            s["error_step"] = "dispatch"
            s["finished_at"] = time.time()
            return s

        self.state_update(run_id, _fail)
        from crack_server.sub_agents import runner

        runner.finish(run_id, "error")
