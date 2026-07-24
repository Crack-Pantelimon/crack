"""Shared HTML renderers for agent trajectories, tool rows, and chat tails.

Moved out of the deleted stages package so chats and sub-agents can render
turns without depending on the harness pipeline.
"""

from __future__ import annotations

import json
import re

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


_TODO_LINE_RE = re.compile(r"^\[(?P<mark>[ xX])\]\s*(?P<rest>.*)$")


def _todo_action_label(args: dict) -> str:
    """Human label for what a todo tool call *did*, read from its input args."""
    action = str(args.get("action") or "").lower()
    if action == "write":
        n = len(args.get("items") or [])
        return f"write · {n} item{'' if n == 1 else 's'}"
    if action == "toggle":
        return f"toggle #{args.get('id')}"
    if action == "list":
        return "list"
    return action or "todo"


def _todo_markdown(output: str) -> str:
    """Turn the tool's ``[ ] #1 text`` lines into a markdown bullet list with
    unicode checkbox glyphs (CommonMark has no task-list plugin), leaving the
    ``Todo list (n/m done):`` header line as-is."""
    lines: list[str] = []
    for raw in output.splitlines():
        m = _TODO_LINE_RE.match(raw.strip())
        if m:
            box = "☑" if m.group("mark").lower() == "x" else "☐"
            lines.append(f"- {box} {m.group('rest')}")
        else:
            lines.append(raw)
    return "\n".join(lines)


def _render_text_action_row(
    kind: str,
    text: str,
    elapsed: float | None = None,
    time_delta: float | None = None,
) -> str:
    """Table row for an assistant text/thinking block: clamped markdown + expand."""
    middle = _render_clamped_markdown(
        text,
        max_lines=5,
        full_label="full text",
        bordered=True,
        collapse_button=True,
    )
    size = f"out {_fmt_chars(len(text))}"
    if elapsed is not None:
        size += f" · {elapsed:.1f}s"
    time_cell = f"{time_delta:.1f}s" if time_delta is not None else ""
    return f"<tr><td>{kind}</td><td>{middle}</td><td>{size}</td><td>{time_cell}</td></tr>"

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


def _render_clamped_markdown(
    md_text: str,
    max_lines: int,
    header: str = "",
    full_label: str = "full output",
    *,
    bordered: bool = False,
    collapse_button: bool = False,
) -> str:
    """First ``max_lines`` lines rendered as markdown, with a ``<details>`` holding
    the full markdown render. ``header`` is emitted above (e.g. the plan badge).

    ``bordered`` / ``collapse_button`` opt into the text/think variant (gray border
    around the expanded body + a bottom ``Collapse ^`` control); spawn/todo rows
    leave them off."""
    lines = md_text.splitlines()
    has_more = len(lines) > max_lines
    head = "\n".join(lines[:max_lines])
    head_html = _ui._render_markdown(head)
    if has_more:
        head_html += '<span class="md-clamp-ellipsis">…</span>'
    body = f'{header}<div class="md-clamp-head">{head_html}</div>'
    if has_more:
        full_html = _ui._render_markdown(md_text)
        full_cls = "md-clamp-full" + (" md-clamp-full--bordered" if bordered else "")
        collapse = (
            '<button type="button" class="md-collapse-btn">Collapse ^</button>'
            if collapse_button
            else ""
        )
        body += (
            f'<details class="md-clamp-more"><summary>{_ui._esc(full_label)}</summary>'
            f'<div class="{full_cls}">{full_html}{collapse}</div></details>'
        )
    return f'<div class="md-clamp">{body}</div>'

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


def _render_tool_action_row(
    block: dict, time_delta: float | None = None
) -> str:
    """Table row for one tool call: type, path/command, in/out char counts, output."""
    esc = _ui._esc
    name = str(block.get("name", "tool"))
    input_raw = block.get("input", "")
    output = str(block.get("output", ""))
    args = _parse_tool_args(input_raw)
    dot = _tool_dot_class(block)
    time_cell = f"{time_delta:.1f}s" if time_delta is not None else ""

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
        action_type = "bash"
        middle = f'<pre class="cmd">{esc(command)}</pre>'
    elif name in ("edit", "write"):
        action_type = name
        path = str(args.get("path") or args.get("filePath") or "")
        middle = f'<code title="{esc(path)}">{esc(_truncate_middle(path))}</code>' if path \
            else f'<pre class="cmd">{esc(str(input_raw))[:400]}</pre>'
    elif name.startswith("spawn_"):
        action_type = esc(name)
        plan_on = bool(args.get("plan"))
        plan_badge = (
            f'<span class="spawn-plan spawn-plan--{"on" if plan_on else "off"}">'
            f'plan {"on" if plan_on else "off"}</span>'
        )
        instructions = str(args.get("instructions") or "")
        middle = _render_clamped_markdown(
            instructions,
            max_lines=7,
            header=plan_badge,
            full_label="full prompt",
        )
    elif name == "todo":
        action_type = "todo"
        out_text = str(block.get("output") or "")
        badge = (
            f'<span class="todo-action">{esc(_todo_action_label(args))}</span>'
        )
        rendered = _ui._render_markdown(_todo_markdown(out_text))
        middle = f'<div class="todo-render">{badge}{rendered}</div>'
        size = f"in {_fmt_chars(len(str(input_raw)))} / out {_fmt_chars(len(output))}"
        elapsed = block.get("elapsed")
        if elapsed is not None:
            size += f" · {elapsed:.1f}s"
        type_cell = (
            f'<span class="tool-dot tool-dot--{dot}" aria-hidden="true"></span>'
            f"{action_type}"
        )
        return (
            f"<tr><td>{type_cell}</td><td>{middle}</td><td>{size}</td>"
            f"<td>{time_cell}</td></tr>"
        )
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
    return (
        f"<tr><td>{type_cell}</td><td>{middle}</td><td>{size}</td>"
        f"<td>{time_cell}</td></tr>"
    )

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


def models_equivalent(a: str, b: str) -> bool:
    """True when two model ids refer to the same model (provider prefix variants)."""
    a = str(a or "").strip()
    b = str(b or "").strip()
    if not a or not b:
        return a == b
    if a == b:
        return True
    return a.endswith("/" + b) or b.endswith("/" + a)


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


# Terminal reason for a whole exchange (why the agent stopped hopping). Rendered
# as a dedicated trajectory row at the bottom of the exchange so the user can see
# *why* the run ended — most importantly a user interruption, which otherwise
# leaves no trace in the trajectory at all.
def render_terminal_reason_row(
    reason: str, duration: float | None = None
) -> str:
    """A trajectory row explaining why the exchange's agent stopped. Empty for
    reasons that are not terminal (``swap``/``time_cap`` continue the run) or that
    already surface elsewhere (``empty`` shows an error card)."""
    after = (
        f" — after {_ui._esc(_ui._format_duration(duration))}"
        if duration is not None
        else ""
    )
    if reason == "waiting_children":
        return (
            '<div class="stage-msg terminal-reason terminal-reason--waiting">'
            '<span class="terminal-reason-line">⏳ Agent ended its turn with sub-agents '
            f"still running — waiting for them (implicit wait_join).{after}</span></div>"
        )
    if reason == "stopped":
        return (
            '<div class="stage-msg terminal-reason terminal-reason--stopped">'
            '<span class="terminal-reason-line">⏹ Stopped by user — run interrupted.'
            f"{after}</span></div>"
        )
    if reason in ("agent_end", "sentinel"):
        return (
            '<div class="stage-msg terminal-reason">'
            '<span class="terminal-reason-line"><small class="muted">■ Agent finished '
            f"its turn — no pending tools or sub-agents.{after}</small></span></div>"
        )
    return ""


def render_error_stop_row(duration: float | None = None) -> str:
    """Red terminal line for an exchange that ended in error (no terminal_reason)."""
    after = (
        f" after {_ui._esc(_ui._format_duration(duration))}"
        if duration is not None
        else ""
    )
    return (
        '<div class="stage-msg stage-error terminal-reason terminal-reason--error">'
        f'<span class="terminal-reason-line">Stopped: Error{after}</span></div>'
    )

def render_prep_timing_row(entry: dict) -> str:
    """UI-only debug line for a preparatory stage (sandbox / first byte / …)."""
    esc = _ui._esc
    label = str(entry.get("label") or entry.get("id") or "prep")
    elapsed = entry.get("elapsed")
    elapsed_s = f"{float(elapsed):.2f}s" if elapsed is not None else "?"
    at = entry.get("at")
    ago = f" · {_ui._format_ago(float(at))}" if at else ""
    return (
        '<div class="stage-msg prep-timing">'
        f'<span class="prep-timing-line"><small class="muted">⏱ {esc(label)}: '
        f"{esc(elapsed_s)}{esc(ago)}</small></span></div>"
    )


def render_note_row(entry: dict) -> str:
    """A UI-only trajectory note (``kind="note"``): a harness-authored marker such
    as a sub-agent returning or a patch being built/applied. Rendered as a thin
    badge line, styled by ``note_type``/``status``, with optional expandable
    ``detail`` (e.g. a failed ``git apply`` stderr). Review-capable patch notes
    expand into the full human-review panel.
    """
    if (
        str(entry.get("note_type") or "") == "patch"
        and str(entry.get("review") or "") in ("1", "true", "yes")
    ):
        return render_patch_review_panel(entry)

    esc = _ui._esc
    note_type = str(entry.get("note_type") or "note")
    status = str(entry.get("status") or "")
    icon = str(entry.get("icon") or "")
    text = str(entry.get("text") or "")
    at = entry.get("at")
    ago = f' <small class="muted">· {_ui._format_ago(float(at))}</small>' if at else ""
    cls = f"traj-note traj-note--{esc(note_type)}"
    if status:
        cls += f" traj-note--{esc(status)}"
    icon_html = f"{esc(icon)} " if icon else ""
    body = (
        f'<div class="stage-msg {cls}">'
        f'<span class="traj-note-line">{icon_html}{esc(text)}{ago}</span>'
    )
    detail = str(entry.get("detail") or "")
    if detail:
        body += (
            '<details class="traj-note-detail"><summary>details</summary>'
            f'<pre class="traj-note-log">{esc(detail)}</pre></details>'
        )
    return body + "</div>"


def render_patch_review_panel(entry: dict) -> str:
    """GitHub-style review panel: diff2html host + Commit/Reject/Ignore actions."""
    from html import escape as html_escape
    from pathlib import Path

    from crack_server import paths

    esc = _ui._esc
    chat_id = str(entry.get("chat_id") or "")
    text = str(entry.get("text") or "Ready for review")
    at = entry.get("at")
    ago = f' <small class="muted">· {_ui._format_ago(float(at))}</small>' if at else ""

    pending: dict = {}
    comments: list = []
    confirm_needed = False
    refreshed = False
    title = ""
    ignored = False
    if chat_id:
        try:
            st = paths.chat_state(chat_id).read()
            pending = dict(st.get("pending_patch") or {})
            comments = list(st.get("review_comments") or [])
            confirm_needed = bool(st.get("review_confirm_needed"))
            refreshed = bool(st.get("review_refreshed_diff"))
            title = str(pending.get("title") or "")
            ignored = bool(pending.get("ignored"))
        except (ValueError, FileNotFoundError):
            pass

    diff_text = ""
    patch_path = Path(str(pending.get("patch_path") or ""))
    if not patch_path.is_file() and chat_id:
        patch_path = paths.chat_dir(chat_id) / "patch.diff"
    if patch_path.is_file():
        try:
            diff_text = patch_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            diff_text = ""

    # Cap huge diffs in the inline payload (collapse behind click via CSS/JS).
    MAX_INLINE = 400_000
    truncated = len(diff_text) > MAX_INLINE
    if truncated:
        diff_text = diff_text[:MAX_INLINE] + "\n\n… (truncated for display) …\n"

    refreshed_banner = ""
    if refreshed or confirm_needed:
        refreshed_banner = (
            '<div class="patch-review-refreshed">'
            "View refreshed — host moved since this patch was built. "
            "Confirm Commit again to integrate the updated merge."
            "</div>"
        )

    if ignored:
        return (
            f'<div class="stage-msg traj-note traj-note--patch">'
            f'<span class="traj-note-line">⏸ Ignored — patch left on disk{ago}</span>'
            f"</div>"
        )

    actions = ""
    if chat_id and pending:
        confirm_field = (
            '<input type="hidden" name="confirm" value="1">' if confirm_needed else ""
        )
        commit_label = "Confirm Commit" if confirm_needed else "Commit"
        actions = f"""
<div class="patch-review-actions">
  <form class="patch-review-commit"
        hx-post="/api/chats/{esc(chat_id)}/patch/commit"
        hx-target="#chat-content" hx-swap="outerHTML">
    {confirm_field}
    <label>Commit message
      <input type="text" name="message" value="{esc(title)}" />
    </label>
    <button type="submit">{esc(commit_label)}</button>
  </form>
  <form class="patch-review-reject"
        hx-post="/api/chats/{esc(chat_id)}/patch/reject"
        hx-target="#chat-content" hx-swap="outerHTML">
    <label>Reject with comments
      <textarea name="message" rows="2"
        placeholder="Feedback for the agent (per-line comments are appended)"></textarea>
    </label>
    <button type="submit" class="secondary">Reject</button>
  </form>
  <form hx-post="/api/chats/{esc(chat_id)}/patch/ignore"
        hx-target="#chat-content" hx-swap="outerHTML">
    <button type="submit" class="contrast">Ignore</button>
  </form>
</div>
"""

    comments_json = html_escape(json.dumps(comments), quote=True)
    # Use a script type that browsers won't execute; Diff2HtmlUI reads textContent.
    diff_payload = html_escape(diff_text)
    panel_id = f"patch-review-{esc(chat_id or 'x')}"

    return f"""
<div class="stage-msg traj-note traj-note--patch patch-review" id="{panel_id}"
     data-chat-id="{esc(chat_id)}"
     data-comments="{comments_json}">
  <div class="patch-review-header">
    <span class="traj-note-line">🔎 {esc(text)}{ago}</span>
    <div class="patch-review-toggles">
      <button type="button" class="patch-view-toggle" data-view="line-by-line">Inline</button>
      <button type="button" class="patch-view-toggle" data-view="side-by-side">Side-by-side</button>
      <button type="button" class="patch-body-toggle">Hide diff</button>
    </div>
  </div>
  {refreshed_banner}
  <script type="application/x-diff" class="patch-review-diff">{diff_payload}</script>
  <div class="patch-review-d2h"></div>
  {actions}
</div>
"""


def new_model_state() -> dict:
    """Mutable tracker threaded through :func:`render_turn_msgs` calls so model
    switches are detected across exchanges (chats) or across a run's hops."""
    return {"model": None, "seen_todo": False, "prev_epoch": None}


def render_actions_table(
    turns: list[dict],
    include_text: bool = True,
    time_delta: float | None = None,
) -> str:
    """Render agent turns as one compact actions table (one row per action).

    Unknown ``kind`` entries (including ``user_prompt``) are skipped — use
    :func:`render_turn_msgs` for per-turn / prompt rows.

    ``time_delta`` (seconds since the previous event) is shown only on the first
    row of each turn so the Time column reads as a per-turn value."""
    rows: list[str] = []
    for turn in turns:
        # Projected trajectory rows carry kind="turn"; only genuinely non-turn
        # entries (user_prompt, error, annotation, …) are skipped here.
        if turn.get("kind") not in (None, "", "turn"):
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
        first = True

        def _delta() -> float | None:
            nonlocal first
            if not first:
                return None
            first = False
            return time_delta

        if thinking:
            rows.append(
                _render_text_action_row("think", thinking, elapsed, time_delta=_delta())
            )
        if include_text and text:
            rows.append(
                _render_text_action_row("text", text, elapsed, time_delta=_delta())
            )
        for block in turn.get("tool_blocks", []):
            rows.append(_render_tool_action_row(block, time_delta=_delta()))
    if not rows:
        return ""
    return (
        '<table class="explore-actions">'
        '<colgroup><col class="col-type"><col class="col-path">'
        '<col class="col-size"><col class="col-time"></colgroup>'
        "<thead><tr>"
        "<th>Type</th><th>Path / command</th><th>Size</th><th>Time</th>"
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


def render_annotation_row(row: dict) -> str:
    """Thin badge row for session / model_change / thinking_level_change."""
    esc = _ui._esc
    label = esc(str(row.get("label") or row.get("ann") or "note"))
    return (
        f'<div class="stage-msg traj-annotation" data-traj-id="{esc(str(row.get("id") or ""))}">'
        f'<small class="muted">{label}</small></div>'
    )


def render_unknown_event_row(row: dict) -> str:
    """Faithful unknown-event row: type label + Expand revealing raw JSON."""
    esc = _ui._esc
    label = esc(str(row.get("label") or "event"))
    raw = row.get("raw")
    try:
        pretty = json.dumps(raw, indent=2, ensure_ascii=False) if raw is not None else ""
    except (TypeError, ValueError):
        pretty = str(raw)
    return (
        f'<div class="stage-msg traj-unknown" data-traj-id="{esc(str(row.get("id") or ""))}">'
        f'<details class="traj-expand">'
        f"<summary><code>{label}</code> · Expand</summary>"
        f'<pre class="traj-raw">{esc(pretty)}</pre>'
        f"</details></div>"
    )


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
        if kind == "annotation":
            # Real model_change events drive the handover divider.
            if entry.get("ann") == "model_change" and model_state is not None:
                cur_model = str(entry.get("model") or "")
                prev = model_state.get("model")
                if prev and cur_model and not models_equivalent(prev, cur_model):
                    swap = bool(model_state.get("seen_todo"))
                    out.append(_model_switch_divider(prev, cur_model, swap))
                    model_state["seen_todo"] = False
                if cur_model:
                    model_state["model"] = cur_model
            out.append(render_annotation_row(entry))
            continue
        if kind == "unknown":
            out.append(render_unknown_event_row(entry))
            continue
        if kind == "ask_user_qa":
            from crack_server import chats
            out.append(chats.render_answered_question(entry.get("qa") or {}))
            continue
        if kind == "terminal_reason":
            dur = entry.get("duration")
            try:
                dur_f = float(dur) if dur is not None else None
            except (TypeError, ValueError):
                dur_f = None
            row = render_terminal_reason_row(
                str(entry.get("reason") or ""), duration=dur_f
            )
            if row:
                out.append(row)
            continue
        if kind == "prep_timing":
            out.append(render_prep_timing_row(entry))
            continue
        if kind == "note":
            out.append(render_note_row(entry))
            continue
        if kind and kind != "turn":
            continue
        # Plain agent turn (kind absent or "turn").
        time_delta: float | None = None
        if model_state is not None:
            from crack_server.trajectory_view import _row_epoch

            epoch = _row_epoch(entry)
            prev_epoch = model_state.get("prev_epoch")
            if epoch is not None and prev_epoch is not None:
                time_delta = max(0.0, float(epoch) - float(prev_epoch))
            if epoch is not None:
                model_state["prev_epoch"] = epoch
        table = render_actions_table(
            [entry], include_text=include_text, time_delta=time_delta
        )
        if not table:
            continue
        cur_model = str(entry.get("model") or "")
        tag = ""
        if model_state is not None and cur_model:
            prev = model_state.get("model")
            if prev and not models_equivalent(prev, cur_model):
                swap = bool(model_state.get("seen_todo"))
                out.append(_model_switch_divider(prev, cur_model, swap))
                model_state["seen_todo"] = False
            model_state["model"] = cur_model
            tag = _model_tag(cur_model)
        if model_state is not None and any(
            b.get("name") == "todo" for b in entry.get("tool_blocks") or []
        ):
            model_state["seen_todo"] = True
        reason_note = _reason_note(str(entry.get("reason") or ""))
        traj_id = _ui._esc(str(entry.get("id") or ""))
        id_attr = f' data-traj-id="{traj_id}"' if traj_id else ""
        out.append(f'<div class="stage-msg"{id_attr}>{tag}{table}{reason_note}</div>')
    return out


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


# The one busy-tail label, shared by chats and sub-agents. Present-participle of
# the agent's persona name (Clanker) — the noun stays "Clanker" in prose.
CLANKING_LABEL = "Clanking…"


def render_running_tail(
    stop_url: str, *, target: str, swap: str = "outerHTML"
) -> str:
    """The shared "agent is working" tail: a ``Clanking…`` spinner + Stop button.

    Lives just above the bottom border of the bordered trajectory area (chats and
    sub-agent cards alike). ``target``/``swap`` are the htmx destination for the
    Stop POST (chat: ``#chat-content``; sub-agent: the run region)."""
    esc = _ui._esc
    return (
        '<div class="stage-running">'
        f"{render_spinner(CLANKING_LABEL)}"
        f'<button class="contrast" hx-post="{esc(stop_url)}" '
        f'hx-target="{esc(target)}" hx-swap="{esc(swap)}">Stop</button>'
        "</div>"
    )


def render_terminal_reason_for_phase(
    phase: str, duration: float | None = None
) -> str:
    """Map a sub-agent run's terminal *phase* onto the same terminal-reason /
    error rows the chat renders from its exchange ``stop_reason`` — so a finished
    sub-agent card ends with the same bottom marker a chat does."""
    if phase == "stopped":
        return render_terminal_reason_row("stopped", duration=duration)
    if phase == "error":
        return render_error_stop_row(duration)
    if phase == "done":
        return render_terminal_reason_row("agent_end", duration=duration)
    return ""


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
    from crack_server import model_latency

    avgs = model_latency.latencies()
    opt_bits: list[str] = []
    for m in options:
        selected = " selected" if m == current else ""
        label = esc(m)
        if m != current and m in avgs:
            label = f"{esc(m)}  ·  {avgs[m]:.1f}s"
        opt_bits.append(f'<option value="{esc(m)}"{selected}>{label}</option>')
    opts = "".join(opt_bits)
    target_attr = f' hx-target="{esc(target)}"' if target else ""
    cont = indent + " " * 8
    inner = indent + " " * 2
    return (
        f'{indent}<select name="{esc(name)}" hx-post="{esc(post_url)}"\n'
        f'{cont}hx-trigger="change"{target_attr} hx-swap="{esc(swap)}">\n'
        f"{inner}{opts}\n"
        f"{indent}</select>"
    )
