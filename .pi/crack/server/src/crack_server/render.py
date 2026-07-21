"""Shared HTML renderers for agent trajectories, tool rows, and chat tails.

Moved out of the deleted stages package so chats and sub-agents can render
turns without depending on the harness pipeline.
"""

from __future__ import annotations

import json
import re
from typing import Callable

from crack_server import models as models_mod
from crack_server import pi_runner
from crack_server import ui as _ui

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


def _render_media_thumbs(block: dict) -> str:
    """Click-to-expand thumbnails for a tool block's persisted ``media`` copies."""
    esc = _ui._esc
    media = block.get("media")
    if not isinstance(media, list) or not media:
        return ""
    thumbs = "".join(
        f'<img class="tool-thumb" src="{esc(str(m.get("url", "")))}" '
        f'alt="{esc(str(m.get("src", "image")))}" title="{esc(str(m.get("src", "")))}">'
        for m in media
        if isinstance(m, dict) and m.get("url")
    )
    return f'<span class="tool-thumbs">{thumbs}</span>' if thumbs else ""


def _tool_dot_class(block: dict) -> str:
    """Classify a tool block: err / ok / pending."""
    if block.get("is_error") is True:
        return "err"
    # Result present (is_error False or output set) → ok.
    if block.get("is_error") is False or block.get("output") not in (None, ""):
        return "ok"
    # toolCall with no result yet → pending.
    return "pending"


# How many lines of a tool's output to show inline before the expand toggle.
_OUTPUT_PREVIEW_LINES = 8


def _render_tool_output(output: str) -> str:
    """Inline output preview (first few lines) with a single expand icon for
    the full result — replaces the old text ``output`` details/summary."""
    esc = _ui._esc
    truncated, marker = pi_runner.truncate_output(output)
    lines = truncated.splitlines()
    head = "\n".join(lines[:_OUTPUT_PREVIEW_LINES])
    has_more = len(lines) > _OUTPUT_PREVIEW_LINES or bool(marker)
    preview = f'<pre class="tool-out-preview">{esc(head)}</pre>'
    if not has_more:
        return f'<div class="tool-out">{preview}</div>'
    full = f'<pre>{esc(truncated)}</pre>'
    if marker:
        full += f'<small class="trunc-marker">{esc(marker)}</small>'
    # Preview stays visible; the details holds only the full output, so its
    # single-icon summary expands the result in place.
    return (
        f'<div class="tool-out">{preview}'
        '<details class="tool-out-more"><summary class="tool-out-toggle" '
        'title="Show full output" aria-label="Show full output"></summary>'
        f"{full}</details></div>"
    )


def _render_tool_action_row(block: dict) -> str:
    """Table row for one tool call: type, path/command, in/out char counts, output."""
    esc = _ui._esc
    name = str(block.get("name", "tool"))
    input_raw = block.get("input", "")
    output = str(block.get("output", ""))
    args = _parse_tool_args(input_raw)
    dot = _tool_dot_class(block)

    if name == "read":
        action_type = "read"
        path = str(args.get("path") or input_raw)
        middle = f'<code title="{esc(path)}">{esc(_truncate_middle(path))}</code>'
        middle += _render_media_thumbs(block)
    elif name == "analyze_image":
        action_type = "analyze_image"
        prompt = str(args.get("prompt") or "")
        bits: list[str] = []
        if prompt:
            bits.append(f'<span class="muted">{esc(prompt)}</span>')
        for p in args.get("image_paths") or []:
            sp = str(p)
            bits.append(f'<code title="{esc(sp)}">{esc(_truncate_middle(sp))}</code>')
        middle = " ".join(bits) if bits else f'<pre class="cmd">{esc(str(input_raw))}</pre>'
        middle += _render_media_thumbs(block)
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
        middle += _render_tool_output(output)

    size = f"in {_fmt_chars(len(str(input_raw)))} / out {_fmt_chars(len(output))}"
    elapsed = block.get("elapsed")
    if elapsed is not None:
        size += f" · {elapsed:.1f}s"
    type_cell = (
        f'<span class="tool-dot tool-dot--{dot}" aria-hidden="true"></span>'
        f"{action_type}"
    )
    return f"<tr><td>{type_cell}</td><td>{middle}</td><td>{size}</td></tr>"


def render_user_prompt_msg(entry: dict) -> str:
    """Expandable `.stage-msg` for a recorded ``user_prompt`` turn entry.

    Collapsed summary is the first line of ``original`` (else ``compiled``),
    plus thumbnails when the entry carries prompt-attachment ``media`` rows.
    Expanded: original message (when present), then a nested details with the
    full compiled prompt verbatim."""
    esc = _ui._esc
    compiled = str(entry.get("compiled") or "")
    original = entry.get("original")
    original_s = str(original) if original not in (None, "") else ""
    label = str(entry.get("label") or "prompt")
    template = str(entry.get("template") or "")
    summary_src = original_s if original_s else compiled
    first_line = summary_src.strip().splitlines()[0] if summary_src.strip() else "(empty)"
    if len(first_line) > 100:
        first_line = first_line[:97] + "…"
    summary = f"user prompt · {label} — {first_line}"
    thumbs = _render_media_thumbs(entry)

    body_parts: list[str] = []
    if original_s:
        body_parts.append(
            '<div class="prompt-original"><strong>original message</strong>'
            f'<pre class="prompt-full">{esc(original_s)}</pre></div>'
        )
    if compiled:
        tmpl_note = f", template {template}" if template else ""
        body_parts.append(
            f'<details class="prompt-compiled"><summary>compiled prompt '
            f"({_fmt_chars(len(compiled))} chars{esc(tmpl_note)})</summary>"
            f'<pre class="prompt-full">{esc(compiled)}</pre></details>'
        )
    if not body_parts:
        body_parts.append(f'<pre class="prompt-full">{esc(summary_src)}</pre>')
    return (
        f'<details class="stage-msg user-prompt-msg">'
        f"<summary>{esc(summary)}{thumbs}</summary>"
        f'{"".join(body_parts)}</details>'
    )


def _model_tag(model: str) -> str:
    """Small per-turn badge naming the model that produced the turn."""
    esc = _ui._esc
    if not model:
        return ""
    return (
        f'<div class="turn-model-row"><span class="turn-model" '
        f'title="ran on {esc(model)}"><code>{esc(model)}</code></span></div>'
    )


def _model_switch_divider(prev: str, cur: str, prewalk_swap: bool) -> str:
    """Full-width marker between turns whenever the model changes.

    ``prewalk_swap`` (a todo list was written before this edit-turn switch)
    labels the automatic planner→implementer handoff; otherwise it's a
    user-initiated switch (a new message sent on a different model)."""
    esc = _ui._esc
    if prewalk_swap:
        text = f"prewalk plan complete — implementing on <code>{esc(cur)}</code>"
    else:
        text = f"switched model → <code>{esc(cur)}</code>"
    return (
        '<div class="stage-msg model-switch">'
        f'<span class="model-switch-line">⇄ {text} '
        f'<small class="muted">(was <code>{esc(prev)}</code>)</small>'
        "</span></div>"
    )


# Hop-end reasons worth a muted note in the trajectory. The mundane natural
# ends (agent_end / sentinel / empty) get nothing; "swap" is already shown by
# the model-switch divider, so it is omitted here to avoid double-labelling.
_REASON_NOTES = {
    "time_cap": "hop hit the time cap — continued on the next turn",
    "stopped": "stopped here",
}


def _reason_note(reason: str) -> str:
    """A small muted note explaining why the hop ended (empty for mundane ends)."""
    label = _REASON_NOTES.get(reason)
    if not label:
        return ""
    return f'<div class="turn-reason"><small class="muted">⏱ {_ui._esc(label)}</small></div>'


def new_model_state() -> dict:
    """Mutable tracker threaded through :func:`render_turn_msgs` calls so model
    switches are detected across exchanges (chats) or across a run's hops."""
    return {"model": None, "seen_todo": False}


def render_actions_table(turns: list[dict], include_text: bool = True) -> str:
    """Render agent turns as one compact actions table (one row per action).

    Unknown ``kind`` entries (including ``user_prompt``) are skipped — use
    :func:`render_turn_msgs` for per-turn / prompt rows."""
    rows: list[str] = []
    for turn in turns:
        if turn.get("kind"):
            continue
        if not (
            turn.get("text", "").strip()
            or turn.get("thinking", "").strip()
            or turn.get("tool_blocks")
        ):
            continue
        thinking = turn.get("thinking", "")
        text = _clean_turn_text(turn.get("text", ""))
        elapsed = turn.get("elapsed")
        if thinking:
            rows.append(_render_text_action_row("think", thinking, elapsed))
        if include_text and text:
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


def _merged_trajectory(turns: list[dict], errors: list[dict]) -> list[dict]:
    """Merge turn/prompt entries with durable error rows, time-ordered.

    Turns keep list order (their ``at`` is monotonic by construction; legacy
    entries without ``at`` inherit the previous entry's key). Errors sort by
    their own ``at`` and, on ties, after the turn(s) they follow — so the
    append-only ``wrap_status`` delta-swap stays consistent."""
    keyed: list[tuple[float, int, int, dict]] = []
    last = 0.0
    for idx, turn in enumerate(turns):
        at = turn.get("at")
        if at is None:
            at = last
        else:
            last = float(at)
        keyed.append((at, 0, idx, turn))
    for idx, err in enumerate(errors):
        keyed.append((float(err.get("at", 0.0)), 1, idx, err))
    keyed.sort(key=lambda item: (item[0], item[1], item[2]))
    return [payload for _, _, _, payload in keyed]


def render_error_row(entry: dict) -> str:
    """A durable `.stage-msg` error row in the trajectory (sibling of the turn
    rows): the ``render_error_msg`` markup plus attempt # and relative time."""
    esc = _ui._esc
    message = str(entry.get("message") or "error")
    detail = str(entry.get("detail") or "")
    meta_bits: list[str] = []
    if entry.get("attempt") is not None:
        meta_bits.append(f"attempt {entry['attempt']}")
    if entry.get("at"):
        meta_bits.append(_ui._format_ago(float(entry["at"])))
    meta = f' <small class="muted">({" · ".join(meta_bits)})</small>' if meta_bits else ""
    html = (
        '<div class="stage-msg stage-error">'
        f'<p class="error-line">⚠ {esc(message)}{meta}</p>'
    )
    if detail:
        html += (
            '<details class="error-detail"><summary>last output (stdout+stderr)</summary>'
            f'<pre class="error-log">{esc(detail)}</pre></details>'
        )
    return html + "</div>"


def render_turn_msgs(
    turns: list[dict],
    errors: list[dict] | None = None,
    include_text: bool = True,
    model_state: dict | None = None,
) -> list[str]:
    """One `.stage-msg` per turn / ``user_prompt`` entry (append-friendly).

    When ``errors`` is given, the durable error rows are interleaved by their
    ``at`` timestamps (see :func:`_merged_trajectory`). Error rows are UI-only:
    agent context never reads them.

    ``model_state`` (a :func:`new_model_state` dict) makes model switches
    visible: each rendered turn carries a small model tag, and a divider row is
    emitted whenever the model changes from the previous turn. Passing the same
    dict across calls (e.g. per-exchange) detects cross-exchange switches too."""
    entries = _merged_trajectory(turns, errors) if errors else list(turns)
    out: list[str] = []
    for entry in entries:
        kind = entry.get("kind")
        if kind == "user_prompt":
            out.append(render_user_prompt_msg(entry))
            continue
        if kind == "error":
            out.append(render_error_row(entry))
            continue
        if kind:
            continue
        table = render_actions_table([entry], include_text=include_text)
        if not table:
            continue
        cur_model = str(entry.get("model") or "")
        tag = ""
        if model_state is not None and cur_model:
            prev = model_state.get("model")
            if prev and prev != cur_model:
                swap = bool(model_state.get("seen_todo"))
                out.append(_model_switch_divider(prev, cur_model, swap))
                # A prewalk swap consumes the todo gate; a later change is a
                # user switch, not another "plan complete".
                model_state["seen_todo"] = False
            model_state["model"] = cur_model
            tag = _model_tag(cur_model)
        if model_state is not None and any(
            b.get("name") == "todo" for b in entry.get("tool_blocks") or []
        ):
            model_state["seen_todo"] = True
        reason_note = _reason_note(str(entry.get("reason") or ""))
        out.append(f'<div class="stage-msg">{tag}{table}{reason_note}</div>')
    return out


def render_turns_trajectory(turns: list[dict], include_text: bool = True) -> str:
    """Joined trajectory HTML (for embedding inside a single details/summary)."""
    return "".join(render_turn_msgs(turns, include_text=include_text))


def render_exchanges(
    exchanges: list[dict],
    render_agent_turns: Callable[[list[dict]], list[str]],
) -> list[str]:
    """Shared chat-exchange walk (plan 4.3 A3): one user-prompt bubble per
    exchange (the recorded ``user_prompt`` entry preferred, else synthesized
    from the raw user text), then the exchange's agent turns via the caller's
    renderer (typically :func:`render_turn_msgs`)."""
    msgs: list[str] = []
    for exchange in exchanges:
        user_text = exchange.get("user", "")
        turns = exchange.get("turns", [])
        media = exchange.get("media", [])
        # An ask_user answer exchange shows the read-only Q&A form the user
        # submitted, not a plain user-message bubble.
        qa = exchange.get("qa")
        if qa:
            from crack_server import chats

            msgs.append(chats.render_answered_question(qa))
            agent_turns = [t for t in turns if t.get("kind") != "user_prompt"]
            errors = exchange.get("errors", [])
            if errors:
                agent_turns = _merged_trajectory(agent_turns, errors)
            if agent_turns:
                msgs.extend(render_agent_turns(agent_turns))
            continue
        prompt_entry = next((t for t in turns if t.get("kind") == "user_prompt"), None)
        if prompt_entry is not None:
            # Prefer recorded compiled prompt; keep original from the exchange.
            entry = dict(prompt_entry)
            entry.setdefault("original", user_text)
            entry.setdefault("label", "chat")
            if media:
                entry.setdefault("media", media)
            msgs.append(render_user_prompt_msg(entry))
        else:
            msgs.append(render_user_prompt_msg({
                "kind": "user_prompt",
                "compiled": "",
                "original": user_text,
                "label": "chat",
                **({"media": media} if media else {}),
            }))
        agent_turns = [t for t in turns if t.get("kind") != "user_prompt"]
        errors = exchange.get("errors", [])
        if errors:
            agent_turns = _merged_trajectory(agent_turns, errors)
        if agent_turns:
            msgs.extend(render_agent_turns(agent_turns))
    return msgs



# ---------------------------------------------------------------------------
# Volatile-tail widgets: errors, spinner (chat tails).
# ---------------------------------------------------------------------------


def render_error_msg(error: str, detail: str = "") -> str:
    """Error card for the volatile tail (not a permanent trajectory msg)."""
    esc = _ui._esc
    html = (
        '<div class="stage-error">'
        f'<p class="error-line">⚠ {esc(error or "error")}</p>'
    )
    if detail:
        html += (
            '<details class="error-detail"><summary>last output (stdout+stderr)</summary>'
            f'<pre class="error-log">{esc(detail)}</pre></details>'
        )
    return html + "</div>"


def render_fatal_error_banner(state: dict) -> str:
    """Prominent banner when the durable error budget is spent. Empty otherwise."""
    over = bool(state.get("error_over_budget")) or len(state.get("errors", [])) >= int(
        state.get("error_budget", pi_runner.MAX_TOTAL_ERRORS)
    )
    if not over:
        return ""
    return (
        '<div class="stage-error stage-error--fatal">'
        '<p class="error-line">⚠ Failed more than 20 times — '
        "something is likely wrong</p></div>"
    )


def render_spinner(label: str) -> str:
    """Busy spinner fragment (no stage-msg wrapper — lives in the tail)."""
    esc = _ui._esc
    return f'<p class="stage-spinner" aria-busy="true">{esc(label)}</p>'


# ---------------------------------------------------------------------------
# Model <select> (settings + unscripted-chat form)
# ---------------------------------------------------------------------------


def model_select(
    name: str,
    current: str,
    post_url: str,
    *,
    swap: str,
    target: str | None = None,
    indent: str = "",
    models: list[str] | None = None,
) -> str:
    """The one model <select> markup: options from the render-safe models
    cache (B21 — never shells out), a saved value kept as an option even when
    missing from the cache, saving on change via hx-post.

    ``models`` (optional) supplies a pre-filtered option list — e.g. the vision
    row passes only image-capable models — instead of the full cache.

    ``indent`` is the select's own leading indent; continuation lines sit 8
    deeper and the options 2 deeper (matching the historic call-site layouts)."""
    esc = _ui._esc
    options = models if models is not None else models_mod.models_for_render()
    if current not in options:
        options = [current] + options
    opts = "".join(
        f'<option value="{esc(m)}"{" selected" if m == current else ""}>{esc(m)}</option>'
        for m in options
    )
    target_attr = f' hx-target="{esc(target)}"' if target else ""
    cont = indent + " " * 8
    inner = indent + " " * 2
    return (
        f'{indent}<select name="{esc(name)}" hx-post="{esc(post_url)}"\n'
        f'{cont}hx-trigger="change"{target_attr} hx-swap="{esc(swap)}">\n'
        f"{inner}{opts}\n"
        f"{indent}</select>"
    )
