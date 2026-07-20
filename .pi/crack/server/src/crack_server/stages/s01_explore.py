"""Stage s01: Explore — hopped, early-stopping repository exploration agent.

Migrated from app.py (behavior-preserving). State stays in explore.json with the
exact same shape; the only producer-side changes are that model ids come from
``model_for(part)`` (harness/explore.json overrides) instead of module constants,
templates load from prompt_templates/explore/, and a successful run auto-starts
the Plan stage.
"""

from __future__ import annotations

import logging
import re
import shlex
import shutil
import subprocess
import time

from crack_server import paths, pi_runner
from crack_server.state import JsonState
from crack_server.stages.base import Part, Stage
from crack_server.stages.render import (
    render_error_msg,
    render_fatal_error_banner,
    render_message_form,
    render_retry_button,
    render_running_tail,
    render_turn_msgs,
)
from crack_server import ui as _ui
from crack_server.ui import _esc
from crack_server.stages.steprun import (
    attach_media_to_blocks,
    error_recorder,
    grant_error_budget,
    record_errors,
    task_prompt_media,
)

logger = logging.getLogger("uvicorn.error")

NANO_MODEL = "nvidia/nemotron-3-nano-30b-a3b"
ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

PI_EXPLORE_TIMEOUT_SECONDS = 3600
EXPLORE_SENTINEL = "EXPLORATION_COMPLETE"
EXPLORE_SIGMAP_MAX_QUERIES = 6
EXPLORE_SIGMAP_MAX_CHARS = 20_000
EXPLORE_RESUME_MESSAGE = "Continue exploring where you left off."



# ---------------------------------------------------------------------------
# Explore-specific render helpers. The compact actions table is shared with the
# other stages (base.render_actions_table); only path-ref rendering is local.
# ---------------------------------------------------------------------------


def _render_path_ref(task_id: str, ref: dict) -> str:
    """Lazy collapsible file reference — contents load on first expand via fileref."""
    from urllib.parse import quote

    rel_path = ref["rel_path"]
    start = ref.get("start")
    end = ref.get("end")

    if start is not None:
        range_str = f"{rel_path}:{start}-{end}" if end and end != start else f"{rel_path}:{start}"
    else:
        range_str = rel_path

    qs = f"path={quote(rel_path, safe='')}"
    if start is not None:
        qs += f"&start={int(start)}"
    if end is not None:
        qs += f"&end={int(end)}"
    return (
        f'<details class="fileref" hx-get="/tasks/{_esc(task_id)}/fileref?{qs}" '
        f'hx-trigger="toggle once" hx-swap="beforeend">'
        f"<summary>{_esc(range_str)}</summary></details>"
    )


# ---------------------------------------------------------------------------
# Explore agent internals (moved from app.py)
# ---------------------------------------------------------------------------


def _persist_explore_turn(task_id: str, current_turn: dict, hop: int) -> None:
    """Append the finished (or partially captured) turn to disk and persist counters."""
    # The sentinel is control signalling, not content — strip it from displayed text.
    text = current_turn.get("text", "").replace(EXPLORE_SENTINEL, "").strip()
    turn = {
        "hop": hop,
        "text": text,
        "thinking": current_turn.get("thinking", "").strip(),
        "tool_blocks": attach_media_to_blocks(
            list(current_turn.get("tool_blocks", [])),
            paths.task_dir(task_id) / "media",
            f"/tasks/{task_id}/media",
        ),
        "elapsed": current_turn.get("elapsed"),
        "at": time.time(),
    }

    def _append(state: dict) -> dict:
        state.setdefault("turns", []).append(turn)
        state["turns_completed"] = pi_runner.count_turn_groups(state["turns"])
        state["hops_completed"] = max(state.get("hops_completed", 0), hop)
        state["path_refs"] = pi_runner.extract_path_refs(_explore_text_for_refs(state))
        return state

    paths.explore_state(task_id).update(_append)


def _explore_text_for_refs(state: dict) -> str:
    """Build a single text corpus used for path-reference extraction."""
    parts = []
    for turn in state.get("turns", []):
        parts.append(turn.get("text", ""))
        parts.append(turn.get("thinking", ""))
        for block in turn.get("tool_blocks", []):
            parts.append(str(block.get("input", "")))
            parts.append(str(block.get("output", "")))
    parts.append(state.get("summary_md", ""))
    return "\n".join(parts)


def _parse_turn_zero_questions(text: str) -> list[str]:
    """Extract the `Q:`-prefixed question lines from turn-zero output (max 10)."""
    questions = []
    for line in text.splitlines():
        line = line.strip()
        if line.upper().startswith("Q:"):
            question = line[2:].strip()
            if question:
                questions.append(question)
    return questions[:10]


def _gate_reply_is_junk(reply: str) -> bool:
    """Detect gate replies that mimic the transcript (fake tool calls / bare commands)
    instead of answering DONE or a bullet list. The gate is biased toward stopping, so
    junk is treated as DONE rather than fed into the next hop."""
    lowered = reply.strip().lower()
    if "<tool_call" in lowered or "<function" in lowered:
        return True
    first_line = lowered.splitlines()[0] if lowered else ""
    return bool(re.match(r"^(sigmap|rg|fd|find|cat|ls|read|bash|echo|cd)\b", first_line))


def _run_sigmap_pre_queries(task_id: str, questions: list[str]) -> str:
    """Run `sigmap ask '<q>'` for the first few turn-zero questions and collect the
    generated `.context/query-context.md` headers into one blob for the hop-1 prompt.

    sigmap is a local CLI (not rate-limited, no upstream to be transient about),
    so it is exempt from the pi transient-retry treatment; failures are logged
    and skipped."""
    root = paths.project_root()
    ctx_path = root / ".context" / "query-context.md"
    blobs: list[str] = []
    for question in questions[:EXPLORE_SIGMAP_MAX_QUERIES]:
        cmd = ["sigmap", "ask", question]
        logger.info("explore sigmap: + %s", shlex.join(cmd))
        try:
            result = subprocess.run(
                cmd, cwd=root, capture_output=True, text=True, timeout=120
            )
        except (OSError, subprocess.TimeoutExpired) as e:
            logger.warning("explore sigmap: failed for %r: %s", question, e)
            continue
        if result.returncode != 0:
            logger.warning(
                "explore sigmap: exited %d for %r: %s",
                result.returncode, question, result.stderr[:200],
            )
            continue
        try:
            blob = ctx_path.read_text(encoding="utf-8")
        except OSError as e:
            logger.warning("explore sigmap: cannot read %s: %s", ctx_path, e)
            continue
        blobs.append(f"### sigmap ask: {question}\n{blob.strip()}")

    context = "\n\n".join(blobs)
    if len(context) > EXPLORE_SIGMAP_MAX_CHARS:
        context = context[:EXPLORE_SIGMAP_MAX_CHARS] + "\n… [sigmap context truncated]"
    return context


# ---------------------------------------------------------------------------
# The stage
# ---------------------------------------------------------------------------


class S01Explore(Stage):
    slug = "explore"
    name = "Explore"
    parts = [
        Part("turn_zero", "Turn zero (question planning)", "turn_zero.md", NANO_MODEL),
        Part("agent", "Explore agent (hops)", "explore.md", ULTRA_MODEL),
        Part("gate", "Between-hop gate", "gate.md", NANO_MODEL),
        Part("summary", "Summary", "explore_summary.md", NANO_MODEL),
    ]

    phase_key = "status"
    message_phase = "running"

    def status(self, task_id: str) -> str:
        s = paths.explore_state(task_id).read().get("status", "idle")
        if s in ("running", "done", "error", "stopped"):
            return s
        return "idle"

    def is_enabled(self, task_id: str) -> bool:
        return True

    def state(self, task_id: str) -> JsonState:
        return paths.explore_state(task_id)

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        """Kick off a background Explore job if one is not already running."""
        explore = paths.explore_state(task_id)
        if explore.read().get("status") == "running":
            return

        # Clear stale hop sessions so a fresh run always chains from a clean slate.
        shutil.rmtree(paths.explore_sessions_dir(task_id), ignore_errors=True)

        fresh = {
            "status": "running",
            "started_at": time.time(),
            "finished_at": None,
            "explored_at": None,
            "prompt_last_modified_at": paths.prompts_last_modified(task_id),
            "stop_reason": None,
            "hops_completed": 0,
            "turns_completed": 0,
            "found_files": 0,
            "questions": [],
            "turns": [],
            "errors": [],
            "error_budget": pi_runner.MAX_TOTAL_ERRORS,
            "path_refs": [],
            "summary_md": "",
            "error": "",
            "stop_requested": False,
        }
        form = self.prepare_start_token(fresh)
        explore.write(fresh)
        self.enqueue_step(task_id, "run", form)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        if step == "run":
            self._run_job(task_id)
        elif step == "resume":
            self._run_job(task_id, resume_message=EXPLORE_RESUME_MESSAGE)
        elif step == "user_message":
            msg = str((form or {}).get("msg", "")).strip()
            self._run_job(task_id, resume_message=msg or EXPLORE_RESUME_MESSAGE)
        else:
            super().run_step(task_id, step, form)

    def _record_prompt_entry(self, task_id: str, entry: dict, label: str, template: str) -> None:
        """Append a compiled-prompt entry into explore.json's turns, in order."""
        entry.setdefault("label", label)
        entry["template"] = template
        media = task_prompt_media(task_id)
        if media:
            entry.setdefault("media", media)

        def _append(state: dict) -> dict:
            state.setdefault("turns", []).append(entry)
            return state

        paths.explore_state(task_id).update(_append)

    def _run_hop(self, task_id: str, hop: int, message: str, start: float, template: str) -> str:
        """One hop via the shared runner, persisting turns into explore.json."""
        return pi_runner.run_agent_hop(
            log_prefix="explore",
            model=self.model_for("agent"),
            session_id=f"explore-{task_id}",
            sessions_dir=paths.explore_sessions_dir(task_id),
            tools="bash,read,mcp,analyze_image",
            message=message,
            start=start,
            sentinel=EXPLORE_SENTINEL,
            timeout_seconds=PI_EXPLORE_TIMEOUT_SECONDS,
            persist_turn=lambda turn, h: _persist_explore_turn(task_id, turn, h),
            hop=hop,
            record_prompt=lambda entry: self._record_prompt_entry(
                task_id, entry, f"hop {hop}", template
            ),
            **self.agent_hop_kwargs(task_id),
        )

    def _read_turn_zero(self, task_id: str) -> str:
        """The persisted turn-zero artefact (questions + example answers), used
        by the gate prompt when a run resumes past the turn-zero step."""
        try:
            return (paths.explore_dir(task_id) / "turn_zero.md").read_text(encoding="utf-8")
        except OSError:
            return ""

    def _run_job(self, task_id: str, resume_message: str | None = None) -> None:
        """Explore run: turn-zero questions → sigmap pre-run → gated hops → summary.

        With ``resume_message`` set (retry-from-error, user message, or a
        requeued job whose turn zero already ran — B5), turn zero and sigmap are
        skipped and the hop loop re-enters on the preserved pi session.

        State is persisted to explore.json after every step, so polling and page
        reloads see live progress; the final summary and turn-zero text are also
        written as markdown artefacts under …/<task>/explore/. A successful run
        auto-starts Plan."""
        start = time.monotonic()
        step_name = "run" if resume_message is None else "resume"
        explore = paths.explore_state(task_id)
        with record_errors(
            explore, step_name, phase_key="status",
            log_message=f"explore worker failed for {task_id}",
        ):
            content = paths.read_all_prompts_joined(task_id)
            mtime = paths.prompts_last_modified(task_id)

            def _stamp(state: dict) -> dict:
                state["prompt_last_modified_at"] = mtime
                return state

            state = explore.update(_stamp)

            if resume_message is None and state.get("questions"):
                # Re-entrant "run" (requeued after a crash): turn zero already
                # happened, so jump straight back into the hop loop (B5).
                resume_message = EXPLORE_RESUME_MESSAGE

            if resume_message is None:
                # --- Turn zero (nano): questions + hallucinated example answers.
                turn_zero_prompt = self.load_template("turn_zero.md").replace("{content}", content)
                turn_zero_text, _ = pi_runner.run_pi_text(
                    turn_zero_prompt,
                    log_prefix="explore-turn-zero",
                    model=self.model_for("turn_zero"),
                    max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
                    record_prompt=lambda entry: self._record_prompt_entry(
                        task_id, entry, "turn_zero", "turn_zero.md"
                    ),
                    record_error=error_recorder(paths.explore_state(task_id)),
                )
                paths.write_explore_artefact(task_id, "turn_zero", turn_zero_text)
                questions = _parse_turn_zero_questions(turn_zero_text)
                logger.info("explore: turn zero produced %d questions", len(questions))

                def _set_questions(state: dict) -> dict:
                    state["questions"] = questions
                    return state

                explore.update(_set_questions)

                # --- sigmap pre-run (local): ranked file-signature headers for hop 1.
                sigmap_context = _run_sigmap_pre_queries(task_id, questions)

                message = (
                    self.load_template("explore.md")
                    .replace("{content}", content)
                    .replace("{questions}", turn_zero_text)
                    .replace("{sigmap_context}", sigmap_context or "(no sigmap context available)")
                )
                template = "explore.md"
            else:
                turn_zero_text = self._read_turn_zero(task_id) or "\n".join(
                    state.get("questions", [])
                )
                message = resume_message
                template = ""

            # --- Hops: unlimited; the between-hop gate is the only hop terminator
            # besides the sentinel, the stage time cap, and an external stop.
            stop_reason = None
            hop = int(explore.read().get("hops_completed", 0))
            while True:
                if time.monotonic() - start > PI_EXPLORE_TIMEOUT_SECONDS:
                    stop_reason = "time_cap"
                    break

                hop += 1
                reason = self._run_hop(task_id, hop, message, start, template)
                template = ""
                if reason == "empty":
                    raise RuntimeError("pi returned empty responses (no content in any turn)")
                if reason == "stopped":
                    self.mark_stopped(task_id)
                    return
                if reason == "sentinel":
                    stop_reason = "sentinel"
                    break
                if reason == "time_cap":
                    stop_reason = "time_cap"
                    break

                # --- Gate (nano): decide whether another hop is warranted.
                state = explore.read()
                gate_template = self.load_template("gate.md")
                transcript = pi_runner.fit_nano_transcript(
                    gate_template,
                    pi_runner.render_transcript_plaintext(state.get("turns", [])),
                    turn_zero_text,
                )
                gate_prompt = gate_template.replace("{questions}", turn_zero_text).replace(
                    "{transcript}", transcript
                )
                gate_reply, _ = pi_runner.run_pi_text(
                    gate_prompt,
                    log_prefix=f"explore-gate-hop{hop}",
                    model=self.model_for("gate"),
                    max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
                    record_prompt=lambda entry: self._record_prompt_entry(
                        task_id, entry, "gate", "gate.md"
                    ),
                    record_error=error_recorder(paths.explore_state(task_id)),
                )
                logger.info("explore: gate after hop %d replied: %r", hop, gate_reply[:200])
                if gate_reply.strip().upper().startswith("DONE"):
                    stop_reason = "gate"
                    break
                if _gate_reply_is_junk(gate_reply):
                    logger.warning(
                        "explore: gate reply looked like a tool call/command; treating as DONE"
                    )
                    stop_reason = "gate"
                    break
                message = (
                    "Continue exploring. Still worth checking:\n"
                    f"{gate_reply}\n\n"
                    f"Emit {EXPLORE_SENTINEL} on its own line once you have enough."
                )

            def _set_stop_reason(state: dict) -> dict:
                state["stop_reason"] = stop_reason
                return state

            state = explore.update(_set_stop_reason)
            logger.info(
                "explore: hops done stop_reason=%s turns=%d elapsed=%.2fs",
                stop_reason, state.get("turns_completed", 0), time.monotonic() - start,
            )

            if not state.get("turns"):
                raise RuntimeError("explore produced no turns")

            # --- Final summarization via a separate, tool-less pi call.
            summary_template = self.load_template("explore_summary.md")
            transcript = pi_runner.fit_nano_transcript(
                summary_template,
                pi_runner.render_transcript_plaintext(state.get("turns", [])),
                content,
            )
            summary_prompt = summary_template.replace("{content}", content).replace(
                "{transcript}", transcript
            )
            summary_md, _ = pi_runner.run_pi_text(
                summary_prompt,
                log_prefix="explore-summary",
                model=self.model_for("summary"),
                max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
                record_prompt=lambda entry: self._record_prompt_entry(
                    task_id, entry, "summary", "explore_summary.md"
                ),
                record_error=error_recorder(paths.explore_state(task_id)),
            )
            paths.write_explore_artefact(task_id, "explore_summary", summary_md)

            def _finish(state: dict) -> dict:
                state["summary_md"] = summary_md
                state["path_refs"] = pi_runner.extract_path_refs(_explore_text_for_refs(state))
                state["found_files"] = len(state["path_refs"])
                state["finished_at"] = time.time()
                state["explored_at"] = state["finished_at"]
                state["status"] = "done"
                return state

            state = explore.update(_finish)
            logger.info(
                "explore: done stop_reason=%s turns=%d found_files=%d",
                stop_reason, len(state.get("turns", [])), state["found_files"],
            )

            # Decision #5: a successful Explore run auto-starts the Plan draft.
            from crack_server import stages

            plan_stage = stages.get("plan")
            if plan_stage is not None:
                plan_stage.start(task_id)

    def retry_from_error(self, task_id: str) -> None:
        """Resume the failed run in place: the preserved pi session under
        explore/sessions/ supplies the context, so nothing is replayed — the
        agent continues exploring where it left off."""
        retry = False

        def _retry(state: dict) -> dict:
            nonlocal retry
            if state.get("status") != "error":
                return state
            retry = True
            state["status"] = "running"
            state["error"] = ""
            state["error_detail"] = ""
            grant_error_budget(state)
            return state

        paths.explore_state(task_id).update(_retry)
        if retry:
            self.enqueue_step(task_id, "resume")

    # -- rendering --------------------------------------------------------------

    def render_section(self, task_id: str) -> str:
        return self.render_status(task_id)

    def render_msgs(self, task_id: str) -> list[str]:
        state = paths.explore_state(task_id).read()
        status = state.get("status", "idle")
        turns = state.get("turns", [])
        summary_md = state.get("summary_md", "")
        questions = state.get("questions", [])
        explored_at = state.get("explored_at")
        stop_reason = state.get("stop_reason")
        path_refs = state.get("path_refs", [])
        msgs: list[str] = []

        if status == "done" and explored_at:
            found = state.get("found_files", len(path_refs))
            meta = f"explored {_ui._format_ago(explored_at)} · {len(turns)} turns · {found} files"
            if stop_reason:
                meta += f" · stop: {_esc(str(stop_reason))}"
            msgs.append(f'<div class="stage-msg explore-meta"><small>{meta}</small></div>')

        if questions:
            items = "".join(f"<li>{_esc(q)}</li>" for q in questions)
            msgs.append(
                f'<details class="stage-msg explore-questions"><summary>Questions ({len(questions)})</summary>'
                f"<ul>{items}</ul></details>"
            )

        msgs.extend(render_turn_msgs(turns, errors=state.get("errors", [])))

        if status == "done" and summary_md:
            msgs.append(
                f'<div class="stage-msg explore-summary">{_ui._render_markdown(summary_md)}</div>'
            )

        if status == "done" and path_refs:
            refs = ['<section class="stage-msg explore-refs"><h3>Referenced files</h3>']
            for ref in path_refs:
                refs.append(_render_path_ref(task_id, ref))
            refs.append("</section>")
            msgs.append("".join(refs))

        return msgs

    def render_tail(self, task_id: str) -> str:
        state = paths.explore_state(task_id).read()
        status = state.get("status", "idle")
        turns = state.get("turns", [])
        summary_md = state.get("summary_md", "")
        error = state.get("error", "")
        path_refs = state.get("path_refs", [])
        explored_at = state.get("explored_at")
        content_id = self.stage_content_id()
        parts: list[str] = []

        if status == "done" and explored_at and paths.prompts_last_modified(task_id) > explored_at:
            parts.append(
                '<div class="explore-stale">Prompts changed since last exploration — Re-explore?</div>'
            )

        # Path refs while still running live in the tail (list grows).
        if status != "done" and path_refs:
            refs = ['<section class="explore-refs"><h3>Referenced files</h3>']
            for ref in path_refs:
                refs.append(_render_path_ref(task_id, ref))
            refs.append("</section>")
            parts.append("".join(refs))

        if status == "error":
            parts.append(render_fatal_error_banner(state))
            parts.append(render_error_msg(error, state.get("error_detail", "")))

        if status != "running":
            label = "Re-explore" if (turns or summary_md) else "Explore"
            buttons = (
                f'<button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">{label}</button>'
            )
            if status == "error":
                buttons += render_retry_button(self, task_id, state.get("error_step"))
            parts.append(f'<div class="stage-buttons">{buttons}</div>')

        if status in ("stopped", "error"):
            parts.append(render_message_form(self, task_id))

        if status == "running":
            parts.append(
                render_running_tail(
                    self,
                    task_id,
                    f'Exploring… turn {state.get("turns_completed", len(turns))}',
                )
            )

        return "".join(parts)

    def render_status(
        self, task_id: str, oob: bool = False, after: int | None = None
    ) -> str:
        return self.wrap_status(
            task_id,
            self.render_msgs(task_id),
            self.render_tail(task_id),
            after=after,
            extra_class="explore-content",
            oob=oob,
        )


STAGE = S01Explore()
