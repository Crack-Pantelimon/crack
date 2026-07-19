"""Shared stage HTML renderers: the agent-trajectory actions table, per-turn
message fragments, and the volatile-tail widgets (error card, spinner, retry /
stop / message buttons) every stage reuses.

Also home of ``model_select``, the one model <select> markup shared by the
stage config screen and the unscripted-chat form. Options always come from the
render-safe models cache (``models.models_for_render``, B21) — rendering never
shells out to ``pi --list-models``.

Stage-typed helpers take the stage duck-type (``action_url``,
``stage_content_id``, ``status``, ``state_read``) and only import
``stages.base`` under TYPE_CHECKING, so this module sits between ui.py (leaf)
and base.py in the import graph with no cycle.
"""

from __future__ import annotations

import json
import re
from typing import TYPE_CHECKING, Callable

from crack_server import models as models_mod
from crack_server import paths
from crack_server import pi_runner
from crack_server import ui as _ui

if TYPE_CHECKING:
    from crack_server.stages.base import Stage

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


def render_user_prompt_msg(entry: dict) -> str:
    """Expandable `.stage-msg` for a recorded ``user_prompt`` turn entry.

    Collapsed summary is the first line of ``original`` (else ``compiled``).
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
        f"<summary>{esc(summary)}</summary>"
        f'{"".join(body_parts)}</details>'
    )


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


def render_turn_msgs(turns: list[dict], include_text: bool = True) -> list[str]:
    """One `.stage-msg` per turn / ``user_prompt`` entry (append-friendly)."""
    out: list[str] = []
    for turn in turns:
        kind = turn.get("kind")
        if kind == "user_prompt":
            out.append(render_user_prompt_msg(turn))
            continue
        if kind:
            continue
        table = render_actions_table([turn], include_text=include_text)
        if table:
            out.append(f'<div class="stage-msg">{table}</div>')
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
    renderer (chats render an Assistant bubble; the Finished stage renders the
    plain trajectory)."""
    msgs: list[str] = []
    for exchange in exchanges:
        user_text = exchange.get("user", "")
        turns = exchange.get("turns", [])
        prompt_entry = next((t for t in turns if t.get("kind") == "user_prompt"), None)
        if prompt_entry is not None:
            # Prefer recorded compiled prompt; keep original from the exchange.
            entry = dict(prompt_entry)
            entry.setdefault("original", user_text)
            entry.setdefault("label", "chat")
            msgs.append(render_user_prompt_msg(entry))
        else:
            msgs.append(render_user_prompt_msg({
                "kind": "user_prompt",
                "compiled": "",
                "original": user_text,
                "label": "chat",
            }))
        agent_turns = [t for t in turns if t.get("kind") != "user_prompt"]
        if agent_turns:
            msgs.extend(render_agent_turns(agent_turns))
    return msgs


# ---------------------------------------------------------------------------
# Volatile-tail widgets: errors, spinner, retry/stop buttons, message form.
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


def render_spinner(label: str) -> str:
    """Busy spinner fragment (no stage-msg wrapper — lives in the tail)."""
    esc = _ui._esc
    return f'<p class="stage-spinner" aria-busy="true">{esc(label)}</p>'


def render_retry_button(stage: "Stage", task_id: str, error_step: str | None) -> str:
    """Continue-from-error button next to a stage's re-run button."""
    if not error_step:
        return ""
    esc = _ui._esc
    content_id = stage.stage_content_id()
    return (
        f'<button hx-post="{esc(stage.action_url(task_id, "retry_error"))}" '
        f'hx-target="#{content_id}" hx-swap="outerHTML" class="secondary retry-error-btn">'
        "Continue from last error</button>"
    )


def _should_show_stop(stage: "Stage", task_id: str) -> bool:
    """Tolerant STOP visibility for stages that have (or had) a stoppable run."""
    if stage.status(task_id) == "running":
        return True
    try:
        state = stage.state_read(task_id)
    except NotImplementedError:
        return False
    if "stop_requested" in state:
        return stage.status(task_id) in ("running", "stopped")
    return paths.stage_pid_file(task_id, stage.slug).is_file()


def render_stop_button(stage: "Stage", task_id: str) -> str:
    """STOP button (tail). Omitted when the stage has no stop support yet."""
    if not _should_show_stop(stage, task_id):
        return ""
    esc = _ui._esc
    return (
        f'<button hx-post="{esc(stage.action_url(task_id, "stop"))}" '
        f'hx-target="#{stage.stage_content_id()}" hx-swap="outerHTML" '
        'class="secondary stop-btn">STOP</button>'
    )


def render_running_tail(stage: "Stage", task_id: str, label: str) -> str:
    """Spinner + STOP side-by-side for a running stage's tail."""
    return f'<div class="stage-running">{render_spinner(label)}{render_stop_button(stage, task_id)}</div>'


def render_message_form(stage: "Stage", task_id: str) -> str:
    """Resume-with-message form for stopped/error tails (chat-form look)."""
    esc = _ui._esc
    return f"""
    <form class="stage-message chat-form" hx-post="{esc(stage.action_url(task_id, "message"))}"
          hx-target="#{stage.stage_content_id()}" hx-swap="outerHTML">
      <label>Send a message to resume the agent
        <textarea name="msg" rows="3" required placeholder="Type a message…"></textarea>
      </label>
      <button type="submit">Send</button>
    </form>
    """


# ---------------------------------------------------------------------------
# Model <select> (stage part rows + unscripted-chat form)
# ---------------------------------------------------------------------------


def model_select(
    name: str,
    current: str,
    post_url: str,
    *,
    swap: str,
    target: str | None = None,
    indent: str = "",
) -> str:
    """The one model <select> markup: options from the render-safe models
    cache (B21 — never shells out), a saved value kept as an option even when
    missing from the cache, saving on change via hx-post.

    ``indent`` is the select's own leading indent; continuation lines sit 8
    deeper and the options 2 deeper (matching the historic call-site layouts)."""
    esc = _ui._esc
    options = models_mod.models_for_render()
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
