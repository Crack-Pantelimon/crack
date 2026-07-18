"""Stage base class: the interface every harness stage implements, plus shared
rendering for the per-stage config screen (/stages/<slug>).

A stage is a named, ordered pipeline step (Explore, Plan, …) with:
- ``parts``: the model-driven pieces of the stage, each with a prompt template
  in ``prompt_templates/<slug>/`` and a configurable model (harness/<slug>.json);
- ``start(task_id)``: kick the stage's background work (idempotent);
- ``render_section`` / ``render_status``: the task-page section and its htmx
  polling fragment.

HTML helpers (_esc, _format_time, _render_base) live in app.py; we import the
module (never names) so the app↔stages import cycle stays safe — attribute
access only ever happens at request time, after both modules are loaded.
"""

from __future__ import annotations

import json
import logging
import re
from dataclasses import dataclass
from pathlib import Path

from fastapi import HTTPException

from crack_server import models as models_mod
from crack_server import paths
from crack_server import pi_runner
from crack_server import app as _ui

logger = logging.getLogger("uvicorn.error")

STATUS_COLORS = {
    "running": "tab--running",
    "awaiting": "tab--running",
    "done": "tab--done",
    "idle": "tab--idle",
    "disabled": "tab--disabled",
    "error": "tab--error",
}

MAX_QUESTIONS_PER_ROUND = 5
_QUESTION_TYPES = ("single", "multiple", "open")
_QUESTIONS_BLOCK_RE = re.compile(r"```questions\s*\n(.*?)```", re.DOTALL)


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

    # -- config (harness/<slug>.json = {"models": {part_key: model_id}}) ------

    def part(self, part_key: str) -> Part:
        for p in self.parts:
            if p.key == part_key:
                return p
        raise KeyError(f"unknown part {part_key!r} for stage {self.slug!r}")

    def model_for(self, part_key: str) -> str:
        """Configured model override, else the Part's default_model."""
        part = self.part(part_key)
        config = paths.read_stage_config(self.slug)
        override = config.get("models", {}).get(part_key)
        return override or part.default_model

    def set_model(self, part_key: str, model_id: str) -> None:
        self.part(part_key)  # validate the part exists
        config = paths.read_stage_config(self.slug)
        config.setdefault("models", {})[part_key] = model_id
        paths.write_stage_config(self.slug, config)

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
        """Tab/glyph status: disabled|idle|running|awaiting|done|error."""
        return "idle"

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

    def enqueue_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        """Enqueue a unit of slow work for the out-of-process worker to run.

        The web process only ever writes fast state + enqueues; all ``pi``
        execution happens in the worker via :meth:`run_step`."""
        from crack_server import queue

        queue.enqueue(task_id, self.slug, step, form)

    def run_step(self, task_id: str, step: str, form: dict | None = None) -> None:
        """Worker dispatch entrypoint: run one enqueued step synchronously.

        Each stage maps ``step`` → its internal ``_run_*`` method. The default
        raises so a misrouted job surfaces loudly in the worker log."""
        raise NotImplementedError(f"{self.slug}: no run_step handler for {step!r}")

    def handle_action(self, action: str, task_id: str, form) -> None:
        """Handle a stage-specific POST action (answers, approve, …)."""
        raise HTTPException(status_code=404, detail=f"unknown action: {action}")

    def render_section(self, task_id: str) -> str:
        return self.render_status(task_id)

    def render_status(self, task_id: str, oob: bool = False) -> str:
        raise NotImplementedError

    def stage_content_id(self) -> str:
        return f"{self.slug}-content"

    def status_poll_url(self, task_id: str) -> str:
        return f"/tasks/{task_id}/stages/{self.slug}/status"

    def start_url(self, task_id: str) -> str:
        return f"/api/tasks/{task_id}/stages/{self.slug}/start"

    def action_url(self, task_id: str, action: str) -> str:
        return f"/api/tasks/{task_id}/stages/{self.slug}/actions/{action}"

    def wrap_status(
        self,
        task_id: str,
        inner: str,
        *,
        msg_count: int,
        polling: bool = False,
        extra_class: str = "",
        oob: bool = False,
    ) -> str:
        """Outer polling wrapper carrying data-stage-status and data-msg-count.

        With ``oob=True`` the content div carries hx-swap-oob so htmx replaces
        the live panel (same id) out-of-band as part of another swap."""
        esc = _ui._esc
        safe_id = esc(task_id)
        status = self.status(task_id)
        poll_attrs = (
            f' hx-trigger="every 1.5s" hx-get="{esc(self.status_poll_url(task_id))}"'
            ' hx-swap="outerHTML"'
            if polling
            else ""
        )
        oob_attr = ' hx-swap-oob="true"' if oob else ""
        cls = f"stage-content {extra_class}".strip()
        return (
            f'<div id="{esc(self.stage_content_id())}" class="{cls}"'
            f' data-stage-status="{esc(status)}" data-msg-count="{msg_count}"'
            f' data-stage-slug="{esc(self.slug)}"{poll_attrs}{oob_attr}>'
            f"{inner}</div>"
        )

    # -- config screen (/stages/<slug>) ----------------------------------------

    def render_part_row(self, part: Part) -> str:
        """One config row: part label, its template, and a model <select> that
        saves on change (target: the row itself, outerHTML)."""
        esc = _ui._esc
        current = self.model_for(part.key)
        options = models_mod.get_models()
        if current not in options:
            options = [current] + options
        opts = "".join(
            f'<option value="{esc(m)}"{" selected" if m == current else ""}>{esc(m)}</option>'
            for m in options
        )
        return f"""
        <div class="part-row">
          <span class="part-label">{esc(part.label)}</span>
          <code>{esc(part.template)}</code>
          <select name="model" hx-post="/api/stages/{esc(self.slug)}/parts/{esc(part.key)}/model"
                  hx-trigger="change" hx-target="closest .part-row" hx-swap="outerHTML">
            {opts}
          </select>
        </div>
        """

    def render_template_row(self, filename: str, editing: bool = False) -> str:
        """Prompt-row style view/edit toggle for one of the stage's templates."""
        esc = _ui._esc
        content = paths.read_stage_template(self.slug, filename)  # raises FileNotFoundError
        stat = (self.template_dir() / filename).stat()
        size = stat.st_size
        mtime = _ui._format_time(stat.st_mtime)

        safe_slug = esc(self.slug)
        safe_name = esc(filename)
        safe_content = esc(content)

        if editing:
            return f"""
            <article class="prompt-row">
              <form hx-put="/api/stages/{safe_slug}/templates/{safe_name}" hx-target="closest article" hx-swap="outerHTML">
                <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">
                  <label style="flex: 1;">Filename <input type="text" value="{safe_name}" readonly></label>
                  <small style="color: #666;">{size} bytes • {mtime}</small>
                </div>
                <label>Content
                  <textarea name="content" rows="12" required>{safe_content}</textarea>
                </label>
                <div class="actions">
                  <button type="submit">Save</button>
                  <button type="button" hx-get="/stages/{safe_slug}/template-row/{safe_name}" hx-target="closest article" hx-swap="outerHTML" class="secondary">Cancel</button>
                </div>
              </form>
            </article>
            """

        return f"""
        <article class="prompt-row">
          <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">
            <span class="name">{safe_name}</span>
            <small style="color: #666;">{size} bytes • {mtime}</small>
          </div>
          <textarea readonly rows="4">{safe_content}</textarea>
          <div class="actions">
            <button hx-get="/stages/{safe_slug}/template-row/{safe_name}?editing=true" hx-target="closest article" hx-swap="outerHTML">Edit</button>
          </div>
        </article>
        """

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


# ---------------------------------------------------------------------------
# Shared Q&A helpers (used by Plan and Plan Review interviewing stages)
# ---------------------------------------------------------------------------


def parse_questions(text: str) -> list[dict]:
    """Extract and validate the last fenced ```questions JSON block (≤5 items)."""
    matches = _QUESTIONS_BLOCK_RE.findall(text)
    if not matches:
        return []
    try:
        raw = json.loads(matches[-1])
    except json.JSONDecodeError:
        logger.warning("questions block is not valid JSON")
        return []
    if not isinstance(raw, list):
        return []

    questions: list[dict] = []
    for item in raw[:MAX_QUESTIONS_PER_ROUND]:
        if not isinstance(item, dict):
            continue
        qid = str(item.get("id", "")).strip()
        qtext = str(item.get("text", "")).strip()
        qtype = str(item.get("type", "")).strip()
        if not qid or not qtext or qtype not in _QUESTION_TYPES:
            continue
        question: dict = {"id": qid, "text": qtext, "type": qtype}
        if qtype in ("single", "multiple"):
            options = item.get("options")
            if not isinstance(options, list) or not options:
                continue
            question["options"] = [str(o) for o in options]
        questions.append(question)
    return questions


def _format_answer(answer) -> str:
    if isinstance(answer, list):
        return ", ".join(str(a) for a in answer)
    return str(answer) if answer else "(no answer)"


def render_qa_history(rounds: list[dict]) -> str:
    """Render answered Q&A rounds as .stage-msg blocks (persisted across refresh)."""
    esc = _ui._esc
    parts: list[str] = []
    for i, rnd in enumerate(rounds, 1):
        answers = rnd.get("answers") or {}
        if not answers:
            continue
        for q in rnd.get("questions", []):
            answer = answers.get(q["id"], "")
            if not answer and answer != 0:
                continue
            parts.append(
                '<div class="stage-msg qa-round">'
                f'<p class="qa-q"><strong>Q:</strong> {esc(str(q["text"]))}</p>'
                f'<p class="qa-a"><strong>A:</strong> {esc(_format_answer(answer))}</p>'
                "</div>"
            )
    return "".join(parts)


def render_questions_form(
    action_url: str,
    target: str,
    round_num: int,
    max_rounds: int | None,
    questions: list[dict],
    *,
    meta: str | None = None,
) -> str:
    """Q&A form with radios/checkboxes/open plus an Other option on single/multiple."""
    esc = _ui._esc
    fields: list[str] = []
    for q in questions:
        qid = str(q["id"])
        safe_qid = esc(qid)
        qtype = q.get("type")
        if qtype in ("single", "multiple"):
            input_type = "radio" if qtype == "single" else "checkbox"
            required = " required" if qtype == "single" else ""
            options = "".join(
                f'<label class="plan-option">'
                f'<input type="{input_type}" name="{safe_qid}" value="{esc(str(o))}"{required}> '
                f"{esc(str(o))}</label>"
                for o in q.get("options", [])
            )
            other_label = (
                f'<label class="plan-option">'
                f'<input type="{input_type}" name="{safe_qid}" value="__other__"> Other,</label>'
                f'<textarea name="{safe_qid}__other" class="other-input" rows="2" disabled'
                f' placeholder="Please specify…"></textarea>'
            )
            control = f'<div class="plan-options">{options}{other_label}</div>'
        else:
            control = f'<textarea name="{safe_qid}" rows="2"></textarea>'
        fields.append(
            f'<fieldset class="plan-question">'
            f"<legend>{esc(str(q['text']))}</legend>"
            f"{control}</fieldset>"
        )

    if meta is None:
        cap = f"/{max_rounds}" if max_rounds else ""
        meta = f"Round {round_num}{cap} — clarification needed:"
    return f"""
    <form class="plan-questions stage-msg" hx-post="{esc(action_url)}"
          hx-target="{esc(target)}" hx-swap="outerHTML">
      <p class="plan-meta"><small>{esc(meta)}</small></p>
      {"".join(fields)}
      <button type="submit">Submit answers</button>
    </form>
    """


def collect_answers(form, questions: list[dict]) -> dict:
    """Collect answers from a FormData, resolving __other__ to free-text."""
    answers: dict = {}
    for q in questions:
        qid = q["id"]
        values = [str(v) for v in form.getlist(qid) if str(v).strip()]
        if q.get("type") == "multiple":
            resolved: list[str] = []
            for v in values:
                if v == "__other__":
                    other = str(form.get(f"{qid}__other", "")).strip()
                    if other:
                        resolved.append(other)
                else:
                    resolved.append(v)
            answers[qid] = resolved
        elif q.get("type") == "single":
            if values and values[0] == "__other__":
                answers[qid] = str(form.get(f"{qid}__other", "")).strip()
            else:
                answers[qid] = values[0] if values else ""
        else:
            answers[qid] = values[0] if values else ""
    return answers


def format_qa_for_prompt(round_entry: dict) -> str:
    """Render one round's questions + answers as Q:/A: pairs for prompts."""
    lines = []
    answers = round_entry.get("answers", {})
    for q in round_entry.get("questions", []):
        lines.append(f"Q: {q['text']}\nA: {_format_answer(answers.get(q['id'], ''))}")
    return "\n\n".join(lines)


# ---------------------------------------------------------------------------
# Shared agent-trajectory rendering — one compact actions table, identical to
# the Explore stage's look (moved here so Plan and Plan Review render the same).
# ---------------------------------------------------------------------------

# Control signalling the agent emits inline (questions blocks / sentinels) is not
# content — strip it from displayed trajectory text so raw JSON never leaks.
_CONTROL_BLOCK_RE = re.compile(r"```questions\s*\n.*?```", re.DOTALL)
_CONTROL_SENTINELS = (
    "READY_TO_PLAN",
    "READY_TO_REVISE",
    "PLAN_REVISED",
    "EXPLORATION_COMPLETE",
)


def _clean_turn_text(text: str) -> str:
    """Remove fenced questions blocks and known control sentinels from turn text."""
    text = _CONTROL_BLOCK_RE.sub("", text)
    for sentinel in _CONTROL_SENTINELS:
        text = text.replace(sentinel, "")
    return text.strip()


def _fmt_chars(n: int) -> str:
    """Compact character count: 240, 1.2k, 12.3k."""
    return f"{n / 1000:.1f}k" if n >= 1000 else str(n)


def _truncate_middle(s: str, max_len: int = 60) -> str:
    """Middle-truncate a path, keeping the head and a whole-segment tail (filename)."""
    if len(s) <= max_len:
        return s
    head_len = max_len // 3
    tail = s[-(max_len - head_len - 1):]
    if "/" in tail:
        tail = tail[tail.index("/"):]
    return s[:head_len] + "…" + tail


def _parse_tool_args(input_raw) -> dict:
    """Tool-call arguments arrive as a dict in pi JSON mode; tolerate JSON strings."""
    if isinstance(input_raw, dict):
        return input_raw
    if isinstance(input_raw, str):
        try:
            parsed = json.loads(input_raw)
        except json.JSONDecodeError:
            return {}
        return parsed if isinstance(parsed, dict) else {}
    return {}


def _render_text_action_row(kind: str, text: str, elapsed: float | None = None) -> str:
    """Table row for an assistant text/thinking block: first-line snippet, expandable."""
    esc = _ui._esc
    stripped = text.strip()
    first_line = stripped.splitlines()[0] if stripped else ""
    snippet = first_line if len(first_line) <= 80 else first_line[:77] + "…"
    if stripped == first_line and len(first_line) <= 80:
        middle = esc(snippet)
    else:
        middle = (
            f"<details><summary>{esc(snippet)}</summary>"
            f'<div class="turn-text">{esc(text)}</div></details>'
        )
    size = f"out {_fmt_chars(len(text))}"
    if elapsed is not None:
        size += f" · {elapsed:.1f}s"
    return f"<tr><td>{kind}</td><td>{middle}</td><td>{size}</td></tr>"


def _render_tool_action_row(block: dict) -> str:
    """Table row for one tool call: type, path/command, in/out char counts, output."""
    esc = _ui._esc
    name = str(block.get("name", "tool"))
    input_raw = block.get("input", "")
    output = str(block.get("output", ""))
    args = _parse_tool_args(input_raw)

    if name == "read":
        action_type = "read"
        path = str(args.get("path") or input_raw)
        middle = f'<code title="{esc(path)}">{esc(_truncate_middle(path))}</code>'
    elif name == "bash":
        command = str(args.get("command") or input_raw)
        action_type = "sigmap" if command.strip().startswith("sigmap") else "bash"
        middle = f'<pre class="cmd">{esc(command)}</pre>'
    elif name in ("edit", "write"):
        action_type = name
        path = str(args.get("path") or args.get("filePath") or "")
        middle = f'<code title="{esc(path)}">{esc(_truncate_middle(path))}</code>' if path \
            else f'<pre class="cmd">{esc(str(input_raw))[:400]}</pre>'
    else:
        action_type = esc(name)
        middle = f'<pre class="cmd">{esc(str(input_raw))}</pre>'

    if output:
        truncated, marker = pi_runner.truncate_output(output)
        body = f"<pre>{esc(truncated)}</pre>"
        if marker:
            body += f'<small class="trunc-marker">{esc(marker)}</small>'
        middle += f"<details><summary>output</summary>{body}</details>"

    size = f"in {_fmt_chars(len(str(input_raw)))} / out {_fmt_chars(len(output))}"
    elapsed = block.get("elapsed")
    if elapsed is not None:
        size += f" · {elapsed:.1f}s"
    return f"<tr><td>{action_type}</td><td>{middle}</td><td>{size}</td></tr>"


def render_actions_table(turns: list[dict]) -> str:
    """Render agent turns as one compact actions table (one row per action).

    Shared by every stage so Explore, Plan, and Plan Review look identical.
    Turn text is cleaned of control blocks/sentinels before display."""
    rows: list[str] = []
    for turn in turns:
        thinking = turn.get("thinking", "")
        text = _clean_turn_text(turn.get("text", ""))
        elapsed = turn.get("elapsed")
        if thinking:
            rows.append(_render_text_action_row("think", thinking, elapsed))
        if text:
            rows.append(_render_text_action_row("text", text, elapsed))
        for block in turn.get("tool_blocks", []):
            rows.append(_render_tool_action_row(block))
    if not rows:
        return ""
    return (
        '<table class="explore-actions"><thead><tr>'
        "<th>Type</th><th>Path / command</th><th>Size</th>"
        f"</tr></thead><tbody>{''.join(rows)}</tbody></table>"
    )


def render_turns_trajectory(turns: list[dict]) -> str:
    """Agent turns as one `.stage-msg`-wrapped actions table (Explore's look)."""
    table = render_actions_table(turns)
    if not table:
        return ""
    return f'<div class="stage-msg">{table}</div>'
