"""Shared Q&A helpers for planner sub-agents (and any chat run that asks).

Agents emit clarifying questions as a fenced ```questions JSON block; these
helpers parse that block, render the questions form, collect htmx submissions,
and format answered rounds back into prompt text.
"""

from __future__ import annotations

import json
import logging
import re

from crack_server import ui as _ui

logger = logging.getLogger("uvicorn.error")

MAX_QUESTIONS_PER_ROUND = 5
_QUESTION_TYPES = ("single", "multiple", "open")
_QUESTIONS_BLOCK_RE = re.compile(r"```questions\s*\n(.*?)```", re.DOTALL)


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


def render_qa_history(rounds: list[dict]) -> list[str]:
    """Render answered Q&A rounds as individual .stage-msg blocks (append-only)."""
    esc = _ui._esc
    parts: list[str] = []
    for rnd in rounds:
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
    return parts


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
