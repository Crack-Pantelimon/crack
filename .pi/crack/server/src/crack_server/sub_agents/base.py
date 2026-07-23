"""Sub-agent persona base class and shared run-step machinery."""

from __future__ import annotations

import asyncio
import logging
import time
import uuid
from pathlib import Path

from crack_server import paths, pi_runner, prewalk, queue, sandbox, titles
from crack_server.ratelimit import MAX_TOTAL_ERRORS, RESUME_MESSAGE
from crack_server.state import JsonState
from crack_server.sub_agents.constants import (
    MAX_DEPTH,
    ORPHAN_PHASE_GRACE_SECONDS,
    SUBAGENT_JOB_SLUG,
    SUBAGENT_TIMEOUT_SECONDS,
)
from crack_server.steprun import (
    TurnPersister,
    error_recorder,
    flush_latencies,
    grant_error_budget,
    prompt_recorder,
)

logger = logging.getLogger("uvicorn.error")

TERMINAL_PHASES = frozenset({"done", "error", "stopped"})
ACTIVE_PHASES = frozenset({"running", "resuming", "writing"})
MAX_NUDGES = 3
# A prewalk coder run may take many edit/verify hops before the report lands.
MAX_HOPS = 25

DEFAULT_MODEL = prewalk.DEFAULT_MODEL


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
        # Fallback display model only. A sub-agent's real model is chosen at
        # spawn time from the global agent settings (planner→implementer for
        # plan mode, the non-plan model otherwise) — never per-persona here.
        return DEFAULT_MODEL

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

    async def astate_read(self, run_id: str) -> dict:
        return await self.state(run_id).aread()

    async def astate_update(self, run_id: str, fn) -> dict:
        return await self.state(run_id).aupdate(fn)

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
            current = (await self.astate_read(run_id)).get("started_token")
            if current != token:
                logger.info(
                    "sub_agent %s: dropping stale job for %s (token mismatch)",
                    self.slug, run_id,
                )
                return None
        successor = await self.run_step(run_id, step, form)
        state = await self.astate_read(run_id)
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
        state = await self.astate_read(run_id)
        if not state.get("title"):
            try:
                title = await asyncio.to_thread(
                    titles.generate_title,
                    state["instructions"],
                    log_prefix=f"subagent-title/{run_id}",
                )
            except Exception:
                logger.exception("sub_agent %s: title generation failed for %s", self.slug, run_id)
                title = ""
            if title:

                def _title(s: dict) -> dict:
                    if not s.get("title"):
                        s["title"] = title
                    return s

                await self.astate_update(run_id, _title)

        def _start(state: dict) -> dict:
            state["phase"] = "running"
            state["hops_completed"] = 0
            state["nudge_count"] = 0
            return state

        await self.astate_update(run_id, _start)
        return await self._run_hop(run_id, None)

    def _compile_message(
        self, run_id: str, form: dict | None, state: dict | None = None
    ) -> tuple[str, str]:
        if state is None:
            state = self.state_read(run_id)
        if form and form.get("user_answer"):
            return str(form["user_answer"]), "user_answer"
        if form and form.get("child_results"):
            return str(form["child_results"]), "child_results"
        if form and form.get("resume"):
            return RESUME_MESSAGE, ""
        if form and form.get("patch_nag"):
            return str(form["patch_nag"]), "patch_nag"
        if form and form.get("patch_conflict"):
            return str(form["patch_conflict"]), "patch_conflict"
        if form and form.get("nudge"):
            # Todo-aware nudge: names the still-open items when a todo list
            # exists, else a generic "keep going" (prewalk.nudge_text). A
            # sub-agent must end by writing its report, so remind it of the path.
            text = prewalk.nudge_text(state.get("turns", []))
            report = state.get("report_path", "")
            if report:
                text += f"\n\nWhen the work is complete, write your report to: {report}"
            return text, "nudge"
        if state.get("hops_completed", 0) > 0:
            return RESUME_MESSAGE, ""
        template = "system.md"
        text = self._fill_template(template, state)
        return text, template

    def _fill_template(self, template: str, state: dict) -> str:
        text = self.load_template(template)
        sub_instr = ""
        if int(state.get("depth", 0)) < MAX_DEPTH:
            try:
                sub_instr = self.load_template("sub_agent_instructions.md")
            except RuntimeError:
                sub_instr = ""
        text = (
            text.replace("{instructions}", state.get("instructions", ""))
            .replace("{report_path}", state.get("report_path", ""))
            .replace("{report_instructions}", self.report_instructions)
            .replace("{sub_agent_instructions}", sub_instr)
        )
        if not sub_instr:
            text = text.replace("\n\n\n", "\n\n")
        return text

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
        state = await self.astate_read(run_id)
        if state.get("phase") in TERMINAL_PHASES:
            return None
        if state.get("stop_requested"):
            await self._amark_stopped(run_id)
            from crack_server.sub_agents import runner

            await asyncio.to_thread(runner.finish, run_id, "stopped")
            return None

        chat_id = state["chat_id"]
        message, template = self._compile_message(run_id, form, state)
        hop_n = int(state.get("hops_completed", 0)) + 1
        state_obj = self.state(run_id)
        persister = TurnPersister(
            state_obj, key="turns",
            media_dir=paths.run_media_dir(chat_id, run_id),
            media_url_prefix=f"/chats/{chat_id}/sub_agents/runs/{run_id}/media",
            conv_id=run_id,
        )
        record = prompt_recorder(persister, f"hop {hop_n}", template, message)

        # Prewalk: pick this hop's model (planner while planning, implementer
        # after the swap, or the single model in non-plan mode) and, while
        # planning, inject the hidden instruction + watch for the first-edit
        # swap. Phase derives from the persisted turns, so a resume is correct.
        turns = state.get("turns", [])
        hop_model = prewalk.model_for_phase(state, turns)
        pw_kwargs = prewalk.hop_prewalk_kwargs(state, turns, self.slug)
        # Stamp this hop's model onto every turn it persists (trajectory swaps).
        persister.current_model = hop_model

        sandbox_name: str | None = None
        if sandbox.sandbox_enabled():
            from crack_server import patch as patch_mod

            # Re-check stop right before touching the container: a Stop that
            # landed after the top-of-hop check tears the container down, and the
            # baseline ops below would then fail against a dead container. Bail
            # cleanly to "stopped" instead of surfacing that as a dispatch error.
            if bool(self.state_read(run_id).get("stop_requested")):
                await self._amark_stopped(run_id)
                from crack_server.sub_agents import runner

                await asyncio.to_thread(runner.finish, run_id, "stopped")
                return None

            parent_conv = (
                state.get("parent_id") if state.get("parent_kind") == "run"
                else state.get("chat_id")
            )
            sandbox_name = await sandbox.ensure_sandbox(
                run_id, parent_conv=str(parent_conv) if parent_conv else None,
            )
            run_directory = paths.run_dir(chat_id, run_id)
            # First hop only (no baseline yet): replay the parent's uncommitted
            # tree into the fresh child sandbox so it starts from the parent's
            # current work (Plan 7 Part A / chain-overlay via git-replay).
            if not patch_mod.base_tree_path(run_directory).is_file():
                await patch_mod.seed_child_from_parent(sandbox_name, run_id, state)
            await patch_mod.ensure_baseline(sandbox_name, run_directory)

        try:
            reason = await pi_runner.arun_agent_hop(
                log_prefix=f"sub_agent/{self.slug}/{run_id}",
                model=hop_model,
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
                sandbox=sandbox_name,
                **pw_kwargs,
            )
        except pi_runner.PiStopped:
            await self._amark_stopped(run_id)
            from crack_server.sub_agents import runner

            await asyncio.to_thread(runner.finish, run_id, "stopped")
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

            await self.astate_update(run_id, _fail)
            from crack_server.sub_agents import runner

            await asyncio.to_thread(runner.finish, run_id, "error")
            return None

        def _bump(state: dict) -> dict:
            state["hops_completed"] = int(state.get("hops_completed", 0)) + 1
            return state

        await self.astate_update(run_id, _bump)
        # Record why this hop ended on its last persisted turn (trajectory note).
        persister.stamp_reason(reason)
        await flush_latencies(persister)

        if reason == "stopped":
            await self._amark_stopped(run_id)
            from crack_server.sub_agents import runner

            await asyncio.to_thread(runner.finish, run_id, "stopped")
            return None

        if reason == "swap":
            # First edit landed under the planner: resume the same session on
            # the implementer model. The phase now derives to "implementing"
            # from the persisted edit turn, pruning the planning instruction.
            started_token = (await self.astate_read(run_id)).get("started_token")
            return ("run", {"run_id": run_id, "started_token": started_token, "resume": True})

        return await self._after_hop(run_id, reason, persister)

    async def _after_hop(
        self, run_id: str, reason: str, persister: TurnPersister
    ) -> tuple[str, dict | None] | None:
        state = await self.astate_read(run_id)
        report = Path(state.get("report_path", ""))
        if report.is_file():
            def _done(s: dict) -> dict:
                s["phase"] = "done"
                s["finished_at"] = time.time()
                return s

            await self.astate_update(run_id, _done)
            from crack_server.sub_agents import runner

            await asyncio.to_thread(runner.finish, run_id, "done")
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

            await self.astate_update(run_id, _nudge)
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

        await self.astate_update(run_id, _fail)
        from crack_server.sub_agents import runner

        await asyncio.to_thread(runner.finish, run_id, "error")
        return None

    def _mark_stopped(self, run_id: str) -> None:
        def _stop(state: dict) -> dict:
            state["phase"] = "stopped"
            state["finished_at"] = time.time()
            return state

        self.state_update(run_id, _stop)

    async def _amark_stopped(self, run_id: str) -> None:
        def _stop(state: dict) -> dict:
            state["phase"] = "stopped"
            state["finished_at"] = time.time()
            return state

        await self.astate_update(run_id, _stop)

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
        if sandbox.sandbox_enabled() and cascade:
            from crack_server import patch as patch_mod

            # Stopped children don't push their patch into the parent overlay.
            patch_mod.extract_run_patch(run_id, forceful=True, mark_pending=False)
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
        # A dispatch that races a Stop (the container was torn down by
        # request_stop) throws here — but that failure is the *expected*
        # consequence of stopping, not a new error. Never clobber a run the user
        # already stopped (or one already terminal): keep it "stopped" so the UI
        # shows a clean interruption instead of a spurious red error + Retry.
        current = self.state_read(run_id)
        if current.get("stop_requested") or current.get("phase") in TERMINAL_PHASES:
            def _ensure_stopped(s: dict) -> dict:
                if s.get("phase") not in TERMINAL_PHASES:
                    s["phase"] = "stopped"
                    s["finished_at"] = time.time()
                return s

            self.state_update(run_id, _ensure_stopped)
            logger.info(
                "sub_agent %s: dispatch failed after stop for %s — kept stopped (%s)",
                self.slug, run_id, message,
            )
            from crack_server.sub_agents import runner

            runner.finish(run_id, "stopped")
            return

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
