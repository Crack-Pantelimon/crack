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
import threading
import time

from crack_server import paths, pi_runner
from crack_server.stages.base import Part, Stage, render_actions_table
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

NANO_MODEL = "nvidia/nemotron-3-nano-30b-a3b"
ULTRA_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

PI_EXPLORE_MAX_TURNS = 15
PI_EXPLORE_TIMEOUT_SECONDS = 300
EXPLORE_MAX_HOPS = 3
EXPLORE_TURNS_PER_HOP = 5
EXPLORE_SENTINEL = "EXPLORATION_COMPLETE"
EXPLORE_SIGMAP_MAX_QUERIES = 6
EXPLORE_SIGMAP_MAX_CHARS = 20_000


def _esc(text: str) -> str:
    return _ui._esc(text)


# ---------------------------------------------------------------------------
# Explore-specific render helpers. The compact actions table is shared with the
# other stages (base.render_actions_table); only path-ref rendering is local.
# ---------------------------------------------------------------------------


def _render_path_ref(ref: dict) -> str:
    """Render a collapsible file reference (collapsed until clicked — the referenced
    line range loads inline when expanded)."""
    rel_path = ref["rel_path"]
    start = ref.get("start")
    end = ref.get("end")

    if start is not None:
        range_str = f"{rel_path}:{start}-{end}" if end and end != start else f"{rel_path}:{start}"
    else:
        range_str = rel_path

    lines, start, end, marker = pi_runner.read_file_lines(
        paths.project_root(), rel_path, start, end
    )
    body = f"<pre>{_esc(lines)}</pre>"
    if marker:
        body += f'<small class="trunc-marker">{_esc(marker)}</small>'
    return f'<details><summary>{_esc(range_str)}</summary>{body}</details>'


# ---------------------------------------------------------------------------
# Explore agent internals (moved from app.py)
# ---------------------------------------------------------------------------


def _persist_explore_turn(task_id: str, current_turn: dict, hop: int) -> None:
    """Append the finished (or partially captured) turn to disk and persist counters."""
    state = paths.read_explore_state(task_id)
    # The sentinel is control signalling, not content — strip it from displayed text.
    text = current_turn.get("text", "").replace(EXPLORE_SENTINEL, "").strip()
    turn = {
        "hop": hop,
        "text": text,
        "thinking": current_turn.get("thinking", "").strip(),
        "tool_blocks": list(current_turn.get("tool_blocks", [])),
    }
    state.setdefault("turns", []).append(turn)
    state["turns_completed"] = len(state["turns"])
    state["hops_completed"] = max(state.get("hops_completed", 0), hop)
    state["path_refs"] = pi_runner.extract_path_refs(_explore_text_for_refs(state))
    paths.write_explore_state(task_id, state)


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
    if "<tool_call" in lowered or "<function" in lowered or "</" in lowered:
        return True
    first_line = lowered.splitlines()[0] if lowered else ""
    return bool(re.match(r"^(sigmap|rg|fd|find|cat|ls|read|bash|echo|cd)\b", first_line))


def _run_sigmap_pre_queries(task_id: str, questions: list[str]) -> str:
    """Run `sigmap ask '<q>'` for the first few turn-zero questions and collect the
    generated `.context/query-context.md` headers into one blob for the hop-1 prompt.

    sigmap is a local CLI (not rate-limited); failures are logged and skipped."""
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

    def status(self, task_id: str) -> str:
        s = paths.read_explore_state(task_id).get("status", "idle")
        if s == "running":
            return "running"
        if s == "done":
            return "done"
        if s == "error":
            return "error"
        return "idle"

    def is_enabled(self, task_id: str) -> bool:
        return True

    # -- lifecycle ------------------------------------------------------------

    def start(self, task_id: str) -> None:
        """Kick off a background Explore job if one is not already running."""
        state = paths.read_explore_state(task_id)
        if state.get("status") == "running":
            return

        # Clear stale hop sessions so a fresh run always chains from a clean slate.
        shutil.rmtree(paths.explore_sessions_dir(task_id), ignore_errors=True)

        paths.write_explore_state(
            task_id,
            {
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
                "path_refs": [],
                "summary_md": "",
                "error": "",
            },
        )
        threading.Thread(target=self._run_job, args=(task_id,), daemon=True).start()

    def _run_hop(self, task_id: str, hop: int, message: str, start: float) -> str:
        """One hop via the shared runner, persisting turns into explore.json."""
        state = paths.read_explore_state(task_id)
        return pi_runner.run_agent_hop(
            log_prefix="explore",
            model=self.model_for("agent"),
            session_id=f"explore-{task_id}",
            sessions_dir=paths.explore_sessions_dir(task_id),
            tools="bash,read",
            message=message,
            start=start,
            sentinel=EXPLORE_SENTINEL,
            turns_per_hop=EXPLORE_TURNS_PER_HOP,
            max_turns=PI_EXPLORE_MAX_TURNS,
            timeout_seconds=PI_EXPLORE_TIMEOUT_SECONDS,
            total_turns=state.get("turns_completed", 0),
            persist_turn=lambda turn, h: _persist_explore_turn(task_id, turn, h),
            hop=hop,
        )

    def _run_job(self, task_id: str) -> None:
        """Hopped Explore run: turn-zero questions → sigmap pre-run → ≤3 gated hops → summary.

        State is persisted to explore.json after every step, so polling and page reloads
        see live progress; the final summary and turn-zero text are also written as
        markdown artefacts under …/<task>/explore/. A successful run auto-starts Plan."""
        start = time.monotonic()
        try:
            content = paths.read_all_prompts_joined(task_id)
            state = paths.read_explore_state(task_id)
            state["prompt_last_modified_at"] = paths.prompts_last_modified(task_id)
            paths.write_explore_state(task_id, state)

            # --- Turn zero (nano): questions + hallucinated example answers.
            turn_zero_prompt = self.load_template("turn_zero.md").replace("{content}", content)
            turn_zero_text = pi_runner.run_pi_text(
                turn_zero_prompt,
                log_prefix="explore-turn-zero",
                model=self.model_for("turn_zero"),
                max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
            )
            paths.write_explore_artefact(task_id, "turn_zero", turn_zero_text)
            questions = _parse_turn_zero_questions(turn_zero_text)
            logger.info("explore: turn zero produced %d questions", len(questions))
            state = paths.read_explore_state(task_id)
            state["questions"] = questions
            paths.write_explore_state(task_id, state)

            # --- sigmap pre-run (local): ranked file-signature headers for hop 1.
            sigmap_context = _run_sigmap_pre_queries(task_id, questions)

            # --- Hops.
            message = (
                self.load_template("explore.md")
                .replace("{content}", content)
                .replace("{questions}", turn_zero_text)
                .replace("{sigmap_context}", sigmap_context or "(no sigmap context available)")
            )
            stop_reason = None
            hop = 0
            while hop < EXPLORE_MAX_HOPS:
                state = paths.read_explore_state(task_id)
                if state.get("turns_completed", 0) >= PI_EXPLORE_MAX_TURNS:
                    stop_reason = "turn_cap"
                    break
                if time.monotonic() - start > PI_EXPLORE_TIMEOUT_SECONDS:
                    stop_reason = "time_cap"
                    break

                hop += 1
                reason = self._run_hop(task_id, hop, message, start)
                if reason == "sentinel":
                    stop_reason = "sentinel"
                    break
                if reason in ("turn_cap", "time_cap"):
                    stop_reason = reason
                    break
                if hop >= EXPLORE_MAX_HOPS:
                    stop_reason = "hop_cap"
                    break

                # --- Gate (nano): decide whether another hop is warranted.
                state = paths.read_explore_state(task_id)
                gate_template = self.load_template("gate.md")
                transcript = pi_runner.fit_nano_transcript(
                    gate_template,
                    pi_runner.render_transcript_plaintext(state.get("turns", [])),
                    turn_zero_text,
                )
                gate_prompt = gate_template.replace("{questions}", turn_zero_text).replace(
                    "{transcript}", transcript
                )
                gate_reply = pi_runner.run_pi_text(
                    gate_prompt,
                    log_prefix=f"explore-gate-hop{hop}",
                    model=self.model_for("gate"),
                    max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
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
                    f"Remember: at most {EXPLORE_TURNS_PER_HOP} tool turns this hop, and emit "
                    f"{EXPLORE_SENTINEL} on its own line once you have enough."
                )

            stop_reason = stop_reason or "hop_cap"
            state = paths.read_explore_state(task_id)
            state["stop_reason"] = stop_reason
            paths.write_explore_state(task_id, state)
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
            summary_md = pi_runner.run_pi_text(
                summary_prompt,
                log_prefix="explore-summary",
                model=self.model_for("summary"),
                max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
            )
            paths.write_explore_artefact(task_id, "explore_summary", summary_md)

            state = paths.read_explore_state(task_id)
            state["summary_md"] = summary_md
            state["path_refs"] = pi_runner.extract_path_refs(_explore_text_for_refs(state))
            state["found_files"] = len(state["path_refs"])
            state["finished_at"] = time.time()
            state["explored_at"] = state["finished_at"]
            state["status"] = "done"
            paths.write_explore_state(task_id, state)
            logger.info(
                "explore: done stop_reason=%s turns=%d found_files=%d",
                stop_reason, len(state.get("turns", [])), state["found_files"],
            )

            # Decision #5: a successful Explore run auto-starts the Plan draft.
            from crack_server import stages

            plan_stage = stages.get("plan")
            if plan_stage is not None:
                plan_stage.start(task_id)
        except Exception as e:
            logger.exception("explore worker failed for %s", task_id)
            state = paths.read_explore_state(task_id)
            state["status"] = "error"
            state["error"] = str(e)
            state["finished_at"] = time.time()
            paths.write_explore_state(task_id, state)

    # -- rendering --------------------------------------------------------------

    def render_section(self, task_id: str) -> str:
        return self.render_status(task_id)

    def render_status(self, task_id: str) -> str:
        """Render Explore content from stored explore.json."""
        safe_id = _esc(task_id)
        state = paths.read_explore_state(task_id)
        status = state.get("status", "idle")
        turns = state.get("turns", [])
        summary_md = state.get("summary_md", "")
        error = state.get("error", "")
        path_refs = state.get("path_refs", [])
        questions = state.get("questions", [])
        explored_at = state.get("explored_at")
        stop_reason = state.get("stop_reason")
        content_id = self.stage_content_id()

        parts: list[str] = []

        if status == "running":
            parts.append(
                '<div class="stage-msg">'
                f'<p aria-busy="true">Exploring… hop {state.get("hops_completed", 0) + 1}/{EXPLORE_MAX_HOPS}'
                f" · turns {len(turns)}/{PI_EXPLORE_MAX_TURNS}</p></div>"
            )
        elif status == "error":
            parts.append(
                f'<div class="stage-msg"><p style="color: #c44;">Error: {_esc(error)}</p></div>'
            )
        elif status == "done" and explored_at:
            found = state.get("found_files", len(path_refs))
            meta = f"explored {_ui._format_ago(explored_at)} · {len(turns)} turns · {found} files"
            if stop_reason:
                meta += f" · stop: {_esc(str(stop_reason))}"
            parts.append(f'<div class="stage-msg explore-meta"><small>{meta}</small></div>')
            if paths.prompts_last_modified(task_id) > explored_at:
                parts.append(
                    '<div class="stage-msg explore-stale">Prompts changed since last exploration — Re-explore?</div>'
                )

        if questions:
            items = "".join(f"<li>{_esc(q)}</li>" for q in questions)
            parts.append(
                f'<details class="stage-msg explore-questions"><summary>Questions ({len(questions)})</summary>'
                f"<ul>{items}</ul></details>"
            )

        if turns:
            parts.append(f'<div class="stage-msg">{render_actions_table(turns)}</div>')

        if status == "done" and summary_md:
            parts.append(
                f'<div class="stage-msg explore-summary">{_ui._render_markdown(summary_md)}</div>'
            )

        if path_refs:
            refs = ['<section class="stage-msg explore-refs"><h3>Referenced files</h3>']
            for ref in path_refs:
                refs.append(_render_path_ref(ref))
            refs.append("</section>")
            parts.append("".join(refs))

        if status != "running":
            label = "Re-explore" if (turns or summary_md) else "Explore"
            parts.append(
                f'<div class="stage-msg"><button hx-post="{self.start_url(task_id)}" '
                f'hx-target="#{content_id}" hx-swap="outerHTML">{label}</button></div>'
            )

        msg_count = max(len(parts), len(turns), 1)
        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=status == "running",
            extra_class="explore-content",
        )


STAGE = S01Explore()
