"""Stage base class: the interface every harness stage implements, plus shared
rendering for the per-stage config screen (/stages/<slug>).

A stage is a named, ordered pipeline step (Explore, Plan, …) with:
- ``parts``: the model-driven pieces of the stage, each with a prompt template
  in ``prompt_templates/<slug>/`` and a configurable model (harness/<slug>.json);
- ``start(task_id)``: kick the stage's background work (idempotent);
- ``render_section`` / ``render_status``: the task-page section and its htmx
  polling fragment.

Split (plan 4.2 A5): Q&A parse/render helpers live in ``stages/qa.py``; the
agent-trajectory renderers, volatile-tail widgets, and the shared model
<select> live in ``stages/render.py``; generic HTML helpers (_esc,
_format_time, _render_base, render_file_row) live in ui.py, a leaf module that
imports neither app nor stages — so there is no import cycle to dodge.
"""

from __future__ import annotations

import logging
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from fastapi import HTTPException

from crack_server import paths
from crack_server import pi_runner
from crack_server import ui as _ui
from crack_server.stages.render import model_select
from crack_server.state import JsonState, task_state_mtimes

logger = logging.getLogger("uvicorn.error")

# A running phase whose state file is younger than this is never flagged as
# orphaned — covers write-state-then-enqueue and complete-then-chain gaps.
ORPHAN_PHASE_GRACE_SECONDS = 10.0

STATUS_COLORS = {
    "running": "tab--running",
    "awaiting": "tab--running",
    "done": "tab--done",
    "idle": "tab--idle",
    "disabled": "tab--disabled",
    "error": "tab--error",
    "stopped": "tab--error",
}


@dataclass(frozen=True)
class Part:
    key: str            # "agent", "gate", "summary", "draft", "final", …
    label: str
    template: str       # template basename within the stage's template dir
    default_model: str


class Stage:
    slug: str = ""
    name: str = ""
    order: int = 0      # parsed from the sNN_ filename by the registry
    parts: list[Part] = []
    # Key in the stage's state dict that carries its lifecycle value ("phase"
    # everywhere except Explore, which predates the phase naming).
    phase_key: str = "phase"
    # Phase written when a user message resumes a stopped/errored stage.
    message_phase: str = "running"

    # -- config (harness/<slug>.json = {"models": {part_key: model_id}}) ------

    def part(self, part_key: str) -> Part:
        for p in self.parts:
            if p.key == part_key:
                return p
        raise KeyError(f"unknown part {part_key!r} for stage {self.slug!r}")

    def model_for(self, part_key: str) -> str:
        """Configured model override, else the Part's default_model."""
        part = self.part(part_key)
        config = paths.stage_config_state(self.slug).read()
        override = config.get("models", {}).get(part_key)
        return override or part.default_model

    def set_model(self, part_key: str, model_id: str) -> None:
        self.part(part_key)  # validate the part exists

        def _set(config: dict) -> dict:
            config.setdefault("models", {})[part_key] = model_id
            return config

        paths.stage_config_state(self.slug).update(_set)

    # -- templates / source ---------------------------------------------------

    def template_dir(self) -> Path:
        return paths.stage_templates_dir(self.slug)

    def source_path(self) -> Path:
        return Path(__file__).resolve().parent / f"s{self.order:02d}_{self.slug}.py"

    def load_template(self, name: str) -> str:
        """Read a template from the stage's template dir fresh on every call."""
        path = self.template_dir() / Path(name).name
        if not path.is_file():
            raise RuntimeError(f"missing prompt template: {path}")
        return path.read_text(encoding="utf-8")

    # -- task-page interface (implemented by subclasses) ----------------------

    def status(self, task_id: str) -> str:
        """Tab/glyph status: disabled|idle|running|awaiting|done|error|stopped."""
        return "idle"

    # -- generic state access (implemented by subclasses) ---------------------

    def state(self, task_id: str) -> JsonState:
        """The stage's per-task state file (subclasses return their JsonState)."""
        raise NotImplementedError(f"{self.slug}: state not implemented")

    def state_read(self, task_id: str) -> dict:
        """Read the stage's per-task state dict (thin wrapper over state().read)."""
        return self.state(task_id).read()

    def state_update(self, task_id: str, fn: Callable[[dict], dict]) -> dict:
        """Read-modify-write the stage's state under its per-path flock (B3)."""
        return self.state(task_id).update(fn)

    def mark_stopped(self, task_id: str) -> None:
        """Persist the stage as cleanly stopped (run_agent_hop returned "stopped")."""
        def _stop(state: dict) -> dict:
            state[self.phase_key] = "stopped"
            return state

        self.state_update(task_id, _stop)

    def is_enabled(self, task_id: str) -> bool:
        """Default gating: previous stage in REGISTRY must be done; first always on."""
        from crack_server import stages

        prev: Stage | None = None
        for stage in stages.REGISTRY:
            if stage.slug == self.slug:
                return prev is None or prev.status(task_id) == "done"
            prev = stage
        return True

    def start(self, task_id: str) -> None:
        """Kick the stage's work. Default: enqueue a ``"start"`` step for the
        worker. Stages that need a fast state write before the slow work runs
        override this to write state then ``enqueue_step``."""
        self.enqueue_step(task_id, "start")

    # -- worker command queue -------------------------------------------------

    def enqueue_step(
        self,
        task_id: str,
        step: str,
        form: dict | None = None,
        ignore_job_id: str | None = None,
    ) -> None:
        """Enqueue a unit of slow work for the out-of-process worker to run.

        The web process only ever writes fast state + enqueues; all ``pi``
        execution happens in the worker via :meth:`run_step`. Enqueueing is
        exclusive per (task, stage): a duplicate while one is pending or in
        flight is dropped (B1 double-run guard). ``ignore_job_id`` exempts the
        caller's own in-flight job from that guard. Steps that need a successor
        step should prefer *returning* ``(step, form)`` from :meth:`run_step`
        — the worker enqueues it after completing the current job."""
        from crack_server import queue

        queue.enqueue_exclusive(task_id, self.slug, step, form, ignore_job_id=ignore_job_id)

    def prepare_start_token(self, state: dict) -> dict:
        """Stamp a fresh ``started_token`` into ``state`` (caller persists it) and
        return the form to pass to :meth:`enqueue_step`, so a stale duplicate
        start job — enqueued before a newer start overwrote the state — exits
        immediately when the worker picks it up (B1)."""
        token = uuid.uuid4().hex
        state["started_token"] = token
        return {"started_token": token}

    def dispatch_step(
        self, task_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        """Worker-side entrypoint: verify the start token (when the job carries
        one) then run the step. Stale start jobs are dropped silently.

        Passes through :meth:`run_step`'s optional successor ``(step, form)``:
        the worker enqueues it only after completing the current job, so a
        stage's own next step never collides with its in-flight processing file
        under the B1 exclusive-enqueue guard."""
        token = (form or {}).get("started_token")
        if token is not None:
            try:
                current = self.state_read(task_id).get("started_token")
            except NotImplementedError:
                current = token
            if current != token:
                logger.info(
                    "%s: dropping stale start job for %s (token mismatch)",
                    self.slug, task_id,
                )
                return None
        return self.run_step(task_id, step, form)

    def run_step(
        self, task_id: str, step: str, form: dict | None = None
    ) -> tuple[str, dict | None] | None:
        """Worker dispatch entrypoint: run one enqueued step synchronously.

        Each stage maps ``step`` → its internal ``_run_*`` method. May return a
        successor ``(step, form)`` for the worker to enqueue after the current
        job completes (the only safe way for a step to chain to its own stage's
        next step). The default raises so a misrouted job surfaces loudly in
        the worker log."""
        raise NotImplementedError(f"{self.slug}: no run_step handler for {step!r}")

    def check_orphaned(self, task_id: str) -> bool:
        """RC6 watchdog: detect a stage stuck in a running phase with no queued
        job behind it (e.g. a dropped or lost enqueue) and land it in ``error``
        instead of an infinite spinner. Returns True when the stage was flipped.

        The grace window covers the write-state-then-enqueue gap and the gap
        between ``queue.complete`` and the deferred successor enqueue: a state
        file younger than the window is never flagged."""
        from crack_server import queue

        try:
            state = self.state(task_id)
        except NotImplementedError:
            return False
        observed = state.read().get(self.phase_key)
        if self.status(task_id) != "running":
            return False
        if queue.has_job(task_id, self.slug):
            return False
        try:
            age = time.time() - state.path.stat().st_mtime
        except OSError:
            return False
        if age < ORPHAN_PHASE_GRACE_SECONDS:
            return False

        flipped = False

        def _fail(s: dict) -> dict:
            nonlocal flipped
            if s.get(self.phase_key) != observed:
                return s  # phase moved on while we were checking — not orphaned
            flipped = True
            s[self.phase_key] = "error"
            s["error"] = (
                "stage was in a running phase with no queued job — "
                "the job was likely dropped or lost; use Retry or restart the stage"
            )
            s.setdefault("error_detail", "")
            s["finished_at"] = time.time()
            return s

        state.update(_fail)
        if flipped:
            logger.error(
                "%s: orphaned running phase %r for %s — no pending/processing job; marked error",
                self.slug, observed, task_id,
            )
        return flipped

    def record_dispatch_error(self, task_id: str, message: str) -> None:
        """Best-effort: land the stage in ``error`` when the worker's dispatch of
        one of its jobs raised outside the stage's own error handling, so the UI
        never spins forever on a silently failed job (B6)."""
        try:
            def _fail(state: dict) -> dict:
                state[self.phase_key] = "error"
                state["error"] = f"worker dispatch failed: {message}"
                state.setdefault("error_detail", "")
                state["finished_at"] = time.time()
                return state

            self.state_update(task_id, _fail)
        except Exception:
            logger.exception("%s: could not record dispatch error for %s", self.slug, task_id)

    def handle_action(self, action: str, task_id: str, form) -> None:
        """Handle a stage-specific POST action (answers, approve, …).

        The generic ``retry_error``, ``stop``, and ``message`` actions are
        handled here for every stage so their buttons/forms work without
        per-stage routing."""
        if action == "retry_error":
            self.retry_from_error(task_id)
            return
        if action == "stop":
            self.request_stop(task_id)
            return
        if action == "message":
            self.post_user_message(task_id, form)
            return
        raise HTTPException(status_code=404, detail=f"unknown action: {action}")

    def request_stop(self, task_id: str) -> None:
        """Generic STOP: flag the stop (so the worker classifies the kill as
        intentional), kill the pi process group, and show ``stopped`` at once."""
        if self.status(task_id) != "running":
            return

        def _flag(state: dict) -> dict:
            state["stop_requested"] = True
            return state

        self.state_update(task_id, _flag)
        killed = pi_runner.kill_pid_file(paths.stage_pid_file(task_id, self.slug))
        logger.info("%s: stop requested for %s (killed=%s)", self.slug, task_id, killed)

        def _stopped(state: dict) -> dict:
            state[self.phase_key] = "stopped"
            return state

        self.state_update(task_id, _stopped)

    def post_user_message(self, task_id: str, form) -> None:
        """Generic continue-with-a-message: allowed from ``stopped`` or ``error``;
        clears the stale error/stop flags (B14) and enqueues a ``user_message``
        step that resumes the stage's pi session with the user's text."""
        msg = str(form.get("msg", "")).strip()
        if not msg:
            return
        accepted = False

        def _resume(state: dict) -> dict:
            nonlocal accepted
            if state.get(self.phase_key) not in ("stopped", "error"):
                return state
            accepted = True
            state["error"] = ""
            state["error_detail"] = ""
            state["stop_requested"] = False
            state[self.phase_key] = self.message_phase
            return state

        self.state_update(task_id, _resume)
        if accepted:
            self.enqueue_step(task_id, "user_message", {"msg": msg})

    def agent_hop_kwargs(self, task_id: str) -> dict:
        """The ``pid_file`` / ``stop_check`` kwargs every stage passes to
        :func:`pi_runner.run_agent_hop` so the generic STOP action works."""
        return {
            "pid_file": paths.stage_pid_file(task_id, self.slug),
            "stop_check": lambda: bool(self.state_read(task_id).get("stop_requested")),
        }

    def retry_from_error(self, task_id: str) -> None:
        """Resume the stage's failed step for another round of retries.

        Overridden by stages that can fail: they flip their error state back to a
        running phase (clearing ``error``/``error_detail``) and re-enqueue the
        recorded ``error_step`` so the agent continues from where it crashed
        (the pi session dir is preserved, so work is not replayed). Default no-op."""
        return None

    def render_section(self, task_id: str) -> str:
        return self.render_status(task_id)

    def render_msgs(self, task_id: str) -> list[str]:
        """Append-only history fragments (one HTML string per `.stage-msg`)."""
        raise NotImplementedError

    def render_tail(self, task_id: str) -> str:
        """Volatile bottom region: spinner, error, forms, buttons."""
        raise NotImplementedError

    def render_status(
        self,
        task_id: str,
        oob: bool = False,
        after: int | None = None,
    ) -> str:
        """Assemble msgs + tail. With ``after``, return only new msgs + OOB tail."""
        return self.wrap_status(
            task_id,
            self.render_msgs(task_id),
            self.render_tail(task_id),
            after=after,
            oob=oob,
        )

    def stage_content_id(self) -> str:
        return f"{self.slug}-content"

    def msgs_id(self) -> str:
        return f"{self.slug}-msgs"

    def tail_id(self) -> str:
        return f"{self.slug}-tail"

    def status_poll_url(self, task_id: str) -> str:
        return f"/tasks/{task_id}/stages/{self.slug}/status"

    def start_url(self, task_id: str) -> str:
        return f"/api/tasks/{task_id}/stages/{self.slug}/start"

    def action_url(self, task_id: str, action: str) -> str:
        return f"/api/tasks/{task_id}/stages/{self.slug}/actions/{action}"

    def _tag_msg(self, index: int, html: str) -> str:
        """Ensure a msg fragment has id="{slug}-msg-{index}" on its outer element."""
        esc = _ui._esc
        msg_id = f"{self.slug}-msg-{index}"
        # Inject id into the first tag when it is already a .stage-msg wrapper.
        for needle in ('<div class="stage-msg', "<div class='stage-msg",
                       '<details class="stage-msg', "<details class='stage-msg",
                       '<form class="stage-msg', "<form class='stage-msg",
                       '<section class="stage-msg', "<section class='stage-msg"):
            if needle in html[:120]:
                tag = needle.split(" ", 1)[0]  # "<div" / "<details" / …
                return html.replace(tag + " ", f'{tag} id="{esc(msg_id)}" ', 1)
        return f'<div id="{esc(msg_id)}" class="stage-msg">{html}</div>'

    def wrap_status(
        self,
        task_id: str,
        msgs: list[str],
        tail: str,
        *,
        after: int | None = None,
        extra_class: str = "",
        oob: bool = False,
    ) -> str:
        """Msgs/tail structure with stable ids for incremental long-poll swaps.

        Full render (``after is None``): content → msgs region + tail region.
        Delta render (``after=n``): only messages with index ``> n``, plus an
        out-of-band outerHTML swap for the tail (and a status meta OOB span).
        """
        esc = _ui._esc
        status = self.status(task_id)
        mtime = task_state_mtimes(task_id)
        tagged = [self._tag_msg(i, m) for i, m in enumerate(msgs)]
        msg_count = len(tagged)
        content_id = esc(self.stage_content_id())
        msgs_id = esc(self.msgs_id())
        tail_id = esc(self.tail_id())
        status_esc = esc(status)
        slug_esc = esc(self.slug)

        if after is not None:
            new_msgs = "".join(tagged[i] for i in range(len(tagged)) if i > after)
            # Tail always refreshes on change; status attrs ride on a meta span
            # that app.js copies onto the live content div (avoid wiping children).
            return (
                new_msgs
                + f'<div id="{tail_id}" hx-swap-oob="outerHTML">{tail}</div>'
                + f'<span id="{slug_esc}-status-meta" hx-swap-oob="outerHTML"'
                + f' data-stage-status="{status_esc}" data-msg-count="{msg_count}"'
                + f' data-state-mtime="{mtime}" hidden></span>'
            )

        oob_attr = ' hx-swap-oob="true"' if oob else ""
        cls = f"stage-content {extra_class}".strip()
        return (
            f'<div id="{content_id}" class="{cls}"'
            f' data-stage-status="{status_esc}" data-msg-count="{msg_count}"'
            f' data-state-mtime="{mtime}" data-stage-slug="{slug_esc}"{oob_attr}>'
            f'<div id="{msgs_id}">{"".join(tagged)}</div>'
            f'<div id="{tail_id}">{tail}</div>'
            f'<span id="{slug_esc}-status-meta" hidden'
            f' data-stage-status="{status_esc}" data-msg-count="{msg_count}"'
            f' data-state-mtime="{mtime}"></span>'
            f"</div>"
        )

    # -- config screen (/stages/<slug>) ----------------------------------------

    def render_part_row(self, part: Part) -> str:
        """One config row: part label, its template, and a model <select> that
        saves on change (target: the row itself, outerHTML)."""
        esc = _ui._esc
        current = self.model_for(part.key)
        select = model_select(
            "model",
            current,
            f"/api/stages/{self.slug}/parts/{part.key}/model",
            target="closest .part-row",
            swap="outerHTML",
            indent=" " * 10,
        )
        return f"""
        <div class="part-row">
          <span class="part-label">{esc(part.label)}</span>
          <code>{esc(part.template)}</code>
{select}
        </div>
        """

    def render_template_row(self, filename: str, editing: bool = False) -> str:
        """Prompt-row style view/edit toggle for one of the stage's templates."""
        content = paths.read_stage_template(self.slug, filename)  # raises FileNotFoundError
        stat = (self.template_dir() / filename).stat()
        meta = f"{stat.st_size} bytes • {_ui._format_time(stat.st_mtime)}"
        return _ui.render_file_row(
            f"/stages/{self.slug}/template-row/{filename}",
            f"/api/stages/{self.slug}/templates/{filename}",
            filename,
            content,
            meta,
            editing,
            indent=" " * 8,
        )

    def render_config_body(self) -> str:
        """Body of the /stages/<slug> page: part model dropdowns, editable
        templates, and the stage's .py source (read-only)."""
        esc = _ui._esc
        part_rows = "".join(self.render_part_row(p) for p in self.parts)

        template_rows = []
        for t in paths.list_stage_templates(self.slug):
            try:
                template_rows.append(self.render_template_row(str(t["name"])))
            except FileNotFoundError:
                continue

        try:
            source = self.source_path().read_text(encoding="utf-8")
        except OSError as e:
            source = f"(could not read source: {e})"

        return f"""
        <header style="margin-bottom: 1.5rem;">
          <h1>Stage: {esc(self.name)}</h1>
          <p style="color: #666; margin: 0;">
            slug <code>{esc(self.slug)}</code> • order {self.order} •
            config <code>harness/{esc(self.slug)}.json</code>
          </p>
          <p><a href="/">← All tasks</a></p>
        </header>

        <section>
          <h2>Parts &amp; models</h2>
          {part_rows}
        </section>

        <section>
          <h2>Prompt templates</h2>
          {"".join(template_rows)}
        </section>

        <section>
          <h2>Source <small style="color: #666;">(read-only)</small></h2>
          <pre class="stage-source">{esc(source)}</pre>
        </section>
        """
