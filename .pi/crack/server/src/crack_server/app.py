"""FastAPI app: HTML editor + JSON API with htmx + pico.css."""

from __future__ import annotations

import html
import json
import logging
import re
import shlex
import shutil
import subprocess
import textwrap
import threading
import time
from pathlib import Path

from fastapi import FastAPI, Form, HTTPException, Query
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles
from markdown_it import MarkdownIt

from crack_server import paths

STATIC_DIR = Path(__file__).resolve().parent / "static"

app = FastAPI(title="crack-pi-server")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

# Use uvicorn's configured logger so INFO messages actually reach the console —
# the root logger has no handler attached under uvicorn's default logging config.
logger = logging.getLogger("uvicorn.error")

# Every model below is hosted behind the nvidia provider, so all three share the
# nvidia-wide 40 calls/minute budget; the title/summary model additionally has its
# own tighter 30 calls/minute budget and a ~4k-token (~10,000 char) input limit.
TITLE_MODEL = "nvidia/nemotron-3-nano-30b-a3b"
EXPLORE_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"
EXPLORE_SUMMARY_MODEL = "nvidia/nemotron-3-nano-30b-a3b"
# Turn-zero (question planning) and the between-hop gate both reuse the cheap nano
# summary model.
EXPLORE_GATE_MODEL = EXPLORE_SUMMARY_MODEL

PI_TIMEOUT_SECONDS = 120
PI_EXPLORE_MAX_TURNS = 15
PI_EXPLORE_TIMEOUT_SECONDS = 300
EXPLORE_MAX_HOPS = 3
EXPLORE_TURNS_PER_HOP = 5
EXPLORE_SENTINEL = "EXPLORATION_COMPLETE"
EXPLORE_SIGMAP_MAX_QUERIES = 6
EXPLORE_SIGMAP_MAX_CHARS = 20_000
EXPLORE_READ_MAX_LINES = 200
EXPLORE_READ_MAX_CHARS = 10_000

NVIDIA_CALLS_PER_MINUTE = 40
TITLE_CALLS_PER_MINUTE = 30
TITLE_MAX_INPUT_CHARS = 10_000

TEMPLATES_DIR = Path(__file__).resolve().parent.parent.parent / "prompt_templates"
_PATH_REF_RE = re.compile(
    r"`?([A-Za-z0-9_][A-Za-z0-9_./-]*\.[A-Za-z]{1,10})`?(?::(\d+)(?:-(\d+))?)?"
)


class _RateLimiter:
    """Thread-safe minimum-interval limiter: converts a calls/minute budget into a
    minimum spacing between calls, and blocks the caller until that spacing has
    elapsed. Holding the lock across the sleep is intentional — it serializes callers
    so the configured spacing is always respected regardless of which thread arrives
    first, which is all a local dev tool needs."""

    def __init__(self, name: str, calls_per_minute: float) -> None:
        self._name = name
        self._min_interval = 60.0 / calls_per_minute
        self._lock = threading.Lock()
        self._last_call = 0.0

    def wait(self) -> None:
        with self._lock:
            now = time.monotonic()
            sleep_for = self._min_interval - (now - self._last_call)
            if sleep_for > 0:
                logger.info("rate-limit(%s): waiting %.2fs", self._name, sleep_for)
                time.sleep(sleep_for)
            self._last_call = time.monotonic()


# One limiter for the shared nvidia-provider budget (applies to every pi call below,
# since every model in use is nvidia-hosted), plus a tighter limiter keyed by model id
# for models with their own additional per-model budget.
_nvidia_limiter = _RateLimiter("nvidia-provider", NVIDIA_CALLS_PER_MINUTE)
_model_limiters: dict[str, _RateLimiter] = {
    TITLE_MODEL: _RateLimiter(f"model:{TITLE_MODEL}", TITLE_CALLS_PER_MINUTE),
}


def _wait_for_rate_limit(model: str) -> None:
    _nvidia_limiter.wait()
    limiter = _model_limiters.get(model)
    if limiter is not None:
        limiter.wait()


def _esc(text: str) -> str:
    return html.escape(text, quote=True)


def _format_time(ts: float) -> str:
    """Format timestamp as YYYY-MM-DD HH:MM."""
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(ts))


def _load_template(name: str) -> str:
    """Read a prompt template from disk fresh on every call (no caching)."""
    path = TEMPLATES_DIR / f"{name}.md"
    if not path.is_file():
        raise RuntimeError(f"missing prompt template: {path}")
    return path.read_text(encoding="utf-8")


# Raw HTML is disabled: anything the model emits renders as escaped text, so the
# summary cannot inject markup into the task page.
_markdown = MarkdownIt("commonmark", {"html": False})


def _render_markdown(md_text: str) -> str:
    """Render markdown to HTML (CommonMark, raw HTML disabled)."""
    return _markdown.render(md_text)


def _format_ago(ts: float) -> str:
    """Human 'X ago' for an epoch timestamp."""
    delta = max(0, int(time.time() - ts))
    if delta < 60:
        return f"{delta}s ago"
    if delta < 3600:
        return f"{delta // 60}m ago"
    if delta < 86400:
        return f"{delta // 3600}h ago"
    return f"{delta // 86400}d ago"


def _fmt_chars(n: int) -> str:
    """Compact character count: 240, 1.2k, 12.3k."""
    return f"{n / 1000:.1f}k" if n >= 1000 else str(n)


def _truncate_middle(s: str, max_len: int = 60) -> str:
    """Middle-truncate a path, keeping the head and a whole-segment tail (filename)."""
    if len(s) <= max_len:
        return s
    head_len = max_len // 3
    tail = s[-(max_len - head_len - 1):]
    # Drop any partial leading segment so the tail starts at a path boundary.
    if "/" in tail:
        tail = tail[tail.index("/"):]
    return s[:head_len] + "…" + tail


def _truncate_output(text: str, max_lines: int = EXPLORE_READ_MAX_LINES, max_chars: int = EXPLORE_READ_MAX_CHARS) -> tuple[str, str | None]:
    """Truncate tool output to max_lines / max_chars (whichever hits first).

    Returns (text, marker); marker is None when nothing was cut."""
    lines = text.splitlines()
    reason = None
    if len(lines) > max_lines:
        text = "\n".join(lines[:max_lines])
        reason = f"{max_lines} lines"
    if len(text) > max_chars:
        text = text[:max_chars]
        reason = f"{max_chars:,} chars"
    if reason is None:
        return text, None
    return text, f"… [truncated at {reason} — ask the agent to read specific line ranges if needed]"


def _tail_truncate(text: str, max_chars: int) -> str:
    """Keep the tail of a long transcript (recent turns matter most to gate/summary)."""
    if len(text) <= max_chars:
        return text
    return "… [earlier transcript omitted]\n" + text[-max_chars:]


def _fit_nano_transcript(template: str, transcript: str, *other_parts: str) -> str:
    """Tail-truncate a transcript so template + other parts + transcript fit the nano
    input limit. The hard cut in `_run_pi_text` would otherwise chop the tail — the
    most recent, most useful turns."""
    used = len(template) + sum(len(p) for p in other_parts) + 200  # safety margin
    return _tail_truncate(transcript, max(2_000, TITLE_MAX_INPUT_CHARS - used))


def _render_base(title: str, body: str, task_id: str | None = None) -> str:
    """Render base HTML template with htmx + pico.css. All page/interaction styling and
    JS lives in static/app.css and static/app.js (linked here, not inlined)."""
    task_attr = f' data-task-id="{_esc(task_id)}"' if task_id else ""
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>{_esc(title)}</title>
  <!-- Pico.css -->
  <link
    rel="stylesheet"
    href="https://cdn.jsdelivr.net/npm/@picocss/pico@2.1.1/css/pico.classless.min.css"
  >
  <link rel="stylesheet" href="/static/app.css">
  <!-- htmx -->
  <script
    src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js"
    integrity="sha384-H5SrcfygHmAuTDZphMHqBJLc3FhssKjG7w/CeCpFReSfwBWDTKpkzPP8c+cLsK+V"
    crossorigin="anonymous"
  ></script>
</head>
<body{task_attr}>
  <main>
    {body}
  </main>
  <script src="/static/app.js"></script>
</body>
</html>"""


def _render_task_card(task_id: str, info: dict) -> str:
    """Render a single task card for the homepage."""
    safe_id = _esc(task_id)
    title = _esc(info.get("title", task_id))
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    return f"""
    <article class="task-card" style="border: 1px solid #ddd; border-radius: 8px; padding: 1rem; margin-bottom: 1rem;">
      <div style="display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem;">
        <div>
          <h3 style="margin: 0 0 0.5rem 0;"><a href="/tasks/{safe_id}" style="text-decoration: none;">{title}</a></h3>
          <small style="color: #666;">ID: {safe_id} • Created: {created} • Modified: {modified}</small>
        </div>
        <form hx-delete="/api/tasks/{safe_id}" hx-confirm="Delete task '{title}'?" hx-target="closest article" hx-swap="outerHTML swap:1s">
          <button type="submit" class="secondary" style="margin: 0;">Delete</button>
        </form>
      </div>
    </article>
    """


def _render_title_h1(task_id: str, title: str, oob: bool = False) -> str:
    """The big page title. Rendered out-of-band (outerHTML on the same id) whenever the
    title changes via slot swaps, so the h1 always tracks the saved value."""
    safe_id = _esc(task_id)
    oob_attr = ' hx-swap-oob="true"' if oob else ""
    return f'<h1 id="title-h1-{safe_id}" style="margin: 0; flex: 1;"{oob_attr}>{_esc(title)}</h1>'


def _render_title_input(task_id: str, title: str) -> str:
    """Render the title <input> alone — the inner content of `#title-slot-{id}`.

    The auto-save (change/blur) targets the slot with innerHTML, never `closest
    header`, so a blur can never clobber the h1 or the Regenerate/Save buttons."""
    safe_id = _esc(task_id)
    safe_title = _esc(title)
    return (
        f'<input type="text" name="title" id="title-input-{safe_id}" class="title-input" '
        f'value="{safe_title}" placeholder="Task title" '
        f'hx-put="/api/tasks/{safe_id}/info" hx-trigger="change delay:500ms, blur" '
        f'hx-target="#title-slot-{safe_id}" hx-swap="innerHTML">'
    )


def _render_title_regen_pending(task_id: str, oob: bool = False) -> str:
    """Slot content shown while a title-regeneration job is running.

    The polling span targets `#title-slot-{id}` (innerHTML), so the h1 and buttons —
    siblings of the slot, outside it — survive every swap. With ``oob=True`` the
    fragment instead carries the slot id + hx-swap-oob so prompt CRUD routes can
    refresh the header out-of-band."""
    safe_id = _esc(task_id)
    current_title = _esc(paths.read_info(task_id).get("title", task_id))
    inner = (
        f'<span class="title-input-pending" aria-busy="true" '
        f'hx-trigger="every 1.5s" hx-get="/tasks/{safe_id}/title-regen-status" '
        f'hx-target="#title-slot-{safe_id}" hx-swap="innerHTML">'
        f'<input type="text" name="title" disabled value="{current_title}">'
        f'<input type="hidden" name="title" value="{current_title}">'
        f'<small>generating title…</small>'
        f'</span>'
    )
    if oob:
        return f'<span id="title-slot-{safe_id}" hx-swap-oob="innerHTML">{inner}</span>'
    return inner


def _render_title_regen_error(task_id: str, error: str) -> str:
    """Terminal state for a failed background title regeneration: restore the normal
    input into the slot plus an inline error note (title attribute has the detail)."""
    safe_error = _esc(error)
    info = paths.read_info(task_id)
    return (
        _render_title_input(task_id, info.get("title", task_id))
        + f'<small class="error" title="{safe_error}">title generation failed</small>'
    )


def _render_task_header(task_id: str, info: dict) -> str:
    """Render the task page header, including the editable title form. This is the only
    title in the UI — prompt rows no longer have their own titles.

    Layout contract: `#title-h1-{id}`, `#title-slot-{id}` and the buttons are
    siblings. Every dynamic title swap (auto-save, regenerate pending/done/error)
    targets the slot with innerHTML and updates the h1 out-of-band, so neither the h1
    nor the buttons can ever be removed by a swap."""
    safe_id = _esc(task_id)
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    title_h1 = _render_title_h1(task_id, info.get("title", task_id))
    title_input = _render_title_input(task_id, info.get("title", task_id))
    return f"""
    <header style="margin-bottom: 1.5rem;">
      <div class="title-row" style="margin-bottom: 1rem;">
        {title_h1}
        <form hx-put="/api/tasks/{safe_id}/info" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML" style="flex: 1; display: flex; gap: 0.5rem; align-items: center;">
          <span id="title-slot-{safe_id}" class="title-slot">{title_input}</span>
          <button type="button" hx-post="/api/tasks/{safe_id}/regenerate-title" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML" class="secondary">Regenerate Title</button>
          <button type="submit" class="secondary">Save</button>
        </form>
      </div>
      <p style="color: #666; margin: 0;">ID: {safe_id} • Created: {created} • Modified: {modified}</p>
      <p><a href="/">← All tasks</a></p>
    </header>
    """


def _render_prompt_row(task_id: str, filename: str, editing: bool = False) -> str:
    """Render one prompt row. View mode always shows the file content (read-only);
    Edit mode swaps the same row (closest article) into an editable form in place."""
    content = paths.read_prompt(task_id, filename)  # raises FileNotFoundError if missing

    stat = (paths.task_dir(task_id) / filename).stat()
    size = stat.st_size
    mtime = _format_time(stat.st_mtime)

    safe_id = _esc(task_id)
    safe_name = _esc(filename)
    safe_content = _esc(content)

    if editing:
        return f"""
        <article class="prompt-row">
          <form hx-put="/api/tasks/{safe_id}/prompts/{safe_name}" hx-target="closest article" hx-swap="outerHTML">
            <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">
              <label style="flex: 1;">Filename <input type="text" value="{safe_name}" readonly></label>
              <small style="color: #666;">{size} bytes • {mtime}</small>
            </div>
            <label>Content
              <textarea name="content" rows="12" required>{safe_content}</textarea>
            </label>
            <div class="actions">
              <button type="submit">Save</button>
              <button type="button" hx-get="/tasks/{safe_id}/prompt-row/{safe_name}" hx-target="closest article" hx-swap="outerHTML" class="secondary">Cancel</button>
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
        <button hx-get="/tasks/{safe_id}/prompt-row/{safe_name}?editing=true" hx-target="closest article" hx-swap="outerHTML">Edit</button>
        <form hx-delete="/api/tasks/{safe_id}/prompts/{safe_name}" hx-target="closest article" hx-swap="outerHTML swap:1s" hx-confirm="Delete '{safe_name}'?" style="margin: 0;">
          <button type="submit" class="secondary" style="color: #c44; border-color: #c44;">Remove</button>
        </form>
      </div>
    </article>
    """


def _render_prompts_section(task_id: str) -> str:
    """Render the full list of prompt rows (always shown, content always viewable)."""
    prompts = paths.list_prompt_files(task_id)
    if not prompts:
        return '<p style="color: #666;">No .md files in this task folder yet.</p>'

    rows = []
    for p in prompts:
        try:
            rows.append(_render_prompt_row(task_id, str(p["name"])))
        except FileNotFoundError:
            continue  # deleted between listing and rendering

    return f"""
    <h2>Prompt files</h2>
    <div id="prompt-list-inner">
      {"".join(rows)}
    </div>
    """


def _run_pi_text(
    prompt: str, log_prefix: str, model: str, max_input_chars: int | None = None
) -> str:
    """Run `pi` non-interactively with a single text prompt.

    Logs the full prompt, exact command line, timeout, elapsed time, and an output
    summary so failures are diagnosable from server logs alone. Raises RuntimeError
    because this helper is only used from background threads, where HTTPException
    has no request context to turn into.
    """
    if max_input_chars is not None and len(prompt) > max_input_chars:
        logger.info(
            "%s: truncating prompt from %d to %d chars", log_prefix, len(prompt), max_input_chars
        )
        prompt = prompt[:max_input_chars]

    cmd = ["pi", "--model", model, "--print", "--no-session", "--no-tools", prompt]

    logger.info("%s: full prompt:\n%s", log_prefix, prompt)
    logger.info("%s: timeout=%ss", log_prefix, PI_TIMEOUT_SECONDS)
    logger.info("+ %s", shlex.join(cmd))

    _wait_for_rate_limit(model)

    start = time.monotonic()
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=PI_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        elapsed = time.monotonic() - start
        logger.error("%s: pi timed out after %.2fs", log_prefix, elapsed)
        raise RuntimeError("pi command timed out")
    except FileNotFoundError:
        elapsed = time.monotonic() - start
        logger.error("%s: pi command not found on PATH (after %.2fs)", log_prefix, elapsed)
        raise RuntimeError("pi command not found")

    elapsed = time.monotonic() - start
    logger.info("%s: pi exited %d in %.2fs", log_prefix, result.returncode, elapsed)

    if result.returncode != 0:
        logger.error("%s: pi stderr:\n%s", log_prefix, result.stderr)
        raise RuntimeError(f"pi command failed: {result.stderr}")

    text = result.stdout.strip()
    logger.info("%s: output summary: %r", log_prefix, text[:200])
    return text


# ---------------------------------------------------------------------------
# Background title regeneration
# ---------------------------------------------------------------------------


def _start_title_regen_job(task_id: str) -> None:
    """Kick off a background title-regeneration job if one is not already running."""
    state = paths.read_title_regen_state(task_id)
    if state.get("status") == "running":
        return

    content = paths.read_all_prompts_joined(task_id)
    if not content:
        paths.write_title_regen_state(
            task_id, {"status": "error", "error": "no prompt files to summarize"}
        )
        return

    paths.write_title_regen_state(task_id, {"status": "running", "started_at": time.time()})
    threading.Thread(
        target=_run_title_regen_worker, args=(task_id, content), daemon=True
    ).start()


def _run_title_regen_worker(task_id: str, content: str) -> None:
    try:
        prompt = _load_template("title").replace("{content}", content)
        title = _run_pi_text(
            prompt,
            log_prefix="regenerate-title",
            model=TITLE_MODEL,
            max_input_chars=TITLE_MAX_INPUT_CHARS,
        )
        paths.write_title_regen_state(task_id, {"status": "done", "title": title})
    except Exception as e:
        logger.exception("regenerate-title worker failed for %s", task_id)
        paths.write_title_regen_state(task_id, {"status": "error", "error": str(e)})


# ---------------------------------------------------------------------------
# Background Explore agent
# ---------------------------------------------------------------------------


def _start_explore_job(task_id: str) -> None:
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
    threading.Thread(target=_run_explore_job, args=(task_id,), daemon=True).start()


def _text_from_content(content) -> str:
    """Extract plain text from a pi message content block (string or list)."""
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    parts = []
    for block in content:
        if isinstance(block, dict) and block.get("type") == "text":
            parts.append(block.get("text", ""))
    return "".join(parts)


def _apply_explore_event_to_turn(event: dict, current_turn: dict) -> None:
    """Accumulate assistant text, thinking, and tool blocks from a pi JSON event.

    We only consume `message_end` events to avoid double-counting deltas; the final
    message carries the complete content for that turn. User messages are skipped.
    Tool results are merged into the matching toolCall block by id.
    """
    etype = event.get("type")
    if etype == "turn_start":
        current_turn.clear()
        current_turn.update({"text": "", "thinking": "", "tool_blocks": []})
        return

    if etype != "message_end":
        return

    message = event.get("message")
    if not isinstance(message, dict):
        return

    role = message.get("role")
    if role == "user":
        return

    if role == "toolResult":
        content = message.get("content", [])
        output = _text_from_content(content)
        tool_call_id = message.get("toolCallId")
        # Merge the result into the matching toolCall block, if present.
        merged = False
        for block in current_turn.get("tool_blocks", []):
            if block.get("id") == tool_call_id:
                block["output"] = output
                merged = True
                break
        if not merged:
            current_turn.setdefault("tool_blocks", []).append(
                {
                    "id": tool_call_id,
                    "name": message.get("toolName", "tool"),
                    "input": "",
                    "output": output,
                }
            )
        return

    # Assistant (or other non-user) message.
    content = message.get("content")
    if not isinstance(content, list):
        return

    for block in content:
        if not isinstance(block, dict):
            continue
        btype = block.get("type")
        if btype == "text":
            current_turn["text"] += block.get("text", "")
        elif btype == "thinking":
            current_turn["thinking"] += block.get("thinking", "")
        elif btype == "toolCall":
            current_turn.setdefault("tool_blocks", []).append(
                {
                    "id": block.get("id"),
                    "name": block.get("name", "tool"),
                    "input": block.get("arguments", block.get("input", "")),
                    "output": "",
                }
            )


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
    state["path_refs"] = _extract_path_refs(_explore_text_for_refs(state))
    paths.write_explore_state(task_id, state)


def _turn_has_content(current_turn: dict) -> bool:
    return bool(
        current_turn.get("text", "").strip()
        or current_turn.get("thinking", "").strip()
        or current_turn.get("tool_blocks")
    )


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


def _resolve_path_ref(root: Path, candidate: str) -> Path | None:
    """Resolve a model-emitted path candidate to a real file under the project root.

    The model emits paths like `workspace/src/lib.rs` or `/workspace/src/lib.rs` even
    though they are relative to the root itself, so normalize before checking:
      1. absolute path starting with str(root) → resolve directly;
      2. leading `root.name + "/"` (e.g. `workspace/…`) → strip it;
      3. plain `root / candidate`.
    First candidate that is an existing file under the root wins."""
    tries: list[Path] = []
    root_str = str(root)
    if candidate == root_str or candidate.startswith(root_str + "/"):
        tries.append(Path(candidate))
    if candidate.startswith(root.name + "/"):
        tries.append(root / candidate[len(root.name) + 1:])
    tries.append(root / candidate)

    for path in tries:
        try:
            resolved = path.resolve()
        except (OSError, RuntimeError):
            continue
        if resolved.is_file() and (resolved == root or root in resolved.parents):
            return resolved
    return None


def _extract_path_refs(text: str) -> list[dict]:
    """Find file-path-looking strings in ``text`` and resolve them under the project root.

    Only references that resolve to real files are kept (unresolvable candidates are
    dropped). Returns dicts with keys ``rel_path``, ``start``, ``end``, deduped on all
    three."""
    root = paths.project_root()
    seen: set[tuple[str, int | None, int | None]] = set()
    refs: list[dict] = []

    for match in _PATH_REF_RE.finditer(text):
        candidate = match.group(1)
        start = int(match.group(2)) if match.group(2) else None
        end = int(match.group(3)) if match.group(3) else start

        abs_path = _resolve_path_ref(root, candidate)
        if abs_path is None:
            continue
        rel_path = abs_path.relative_to(root).as_posix()

        key = (rel_path, start, end)
        if key in seen:
            continue
        seen.add(key)

        refs.append({"rel_path": rel_path, "start": start, "end": end})

    return refs


def _read_file_lines(root: Path, rel_path: str, start: int | None, end: int | None) -> tuple[str, int, int, str | None]:
    """Read a clamped line range from a project file.

    Returns (text, start, end, truncation_marker). The range is capped at
    EXPLORE_READ_MAX_LINES lines and the text at EXPLORE_READ_MAX_CHARS chars."""
    path = root / rel_path
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return "", 0, 0, None

    n = len(lines)
    if start is None or start < 1:
        start = 1
    if end is None or end < start:
        end = start + 49
    if start > n:
        start = n
    if end > n:
        end = n
    if end - start + 1 > EXPLORE_READ_MAX_LINES:
        end = start + EXPLORE_READ_MAX_LINES - 1

    text, marker = _truncate_output("\n".join(lines[start - 1 : end]))
    return text, start, end, marker


def _render_transcript_plaintext(turns: list[dict]) -> str:
    """Render a plaintext transcript of the explore turns for gate/summary prompts."""
    parts = []
    for i, turn in enumerate(turns, 1):
        parts.append(f"--- Turn {i} (hop {turn.get('hop', 1)}) ---")
        if turn.get("text"):
            parts.append(turn["text"])
        if turn.get("thinking"):
            parts.append("Thinking:\n" + turn["thinking"])
        for block in turn.get("tool_blocks", []):
            name = block.get("name", "tool")
            if block.get("input") not in (None, ""):
                parts.append(f"Tool {name}: {block['input']}")
            if block.get("output") not in (None, ""):
                parts.append(f"Result:\n{block['output']}")
    return "\n\n".join(parts)


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


def _render_text_action_row(kind: str, text: str) -> str:
    """Table row for an assistant text/thinking block: first-line snippet, expandable."""
    stripped = text.strip()
    first_line = stripped.splitlines()[0] if stripped else ""
    snippet = first_line if len(first_line) <= 80 else first_line[:77] + "…"
    if stripped == first_line and len(first_line) <= 80:
        middle = _esc(snippet)
    else:
        middle = (
            f"<details><summary>{_esc(snippet)}</summary>"
            f'<div class="turn-text">{_esc(text)}</div></details>'
        )
    size = f"out {_fmt_chars(len(text))}"
    return f"<tr><td>{kind}</td><td>{middle}</td><td>{size}</td></tr>"


def _render_tool_action_row(block: dict) -> str:
    """Table row for one tool call: type, path/command, in/out char counts, output."""
    name = str(block.get("name", "tool"))
    input_raw = block.get("input", "")
    output = str(block.get("output", ""))
    args = _parse_tool_args(input_raw)

    if name == "read":
        action_type = "read"
        path = str(args.get("path") or input_raw)
        middle = f'<code title="{_esc(path)}">{_esc(_truncate_middle(path))}</code>'
    elif name == "bash":
        command = str(args.get("command") or input_raw)
        action_type = "sigmap" if command.strip().startswith("sigmap") else "bash"
        middle = f'<pre class="cmd">{_esc(command)}</pre>'
    else:
        action_type = _esc(name)
        middle = f'<pre class="cmd">{_esc(str(input_raw))}</pre>'

    if output:
        truncated, marker = _truncate_output(output)
        body = f"<pre>{_esc(truncated)}</pre>"
        if marker:
            body += f'<small class="trunc-marker">{_esc(marker)}</small>'
        middle += f"<details><summary>output</summary>{body}</details>"

    size = f"in {_fmt_chars(len(str(input_raw)))} / out {_fmt_chars(len(output))}"
    return f"<tr><td>{action_type}</td><td>{middle}</td><td>{size}</td></tr>"


def _render_explore_actions(turns: list[dict]) -> str:
    """Render all explore turns as one compact table — one row per action."""
    rows: list[str] = []
    for turn in turns:
        thinking = turn.get("thinking", "")
        text = turn.get("text", "")
        if thinking:
            rows.append(_render_text_action_row("think", thinking))
        if text:
            rows.append(_render_text_action_row("text", text))
        for block in turn.get("tool_blocks", []):
            rows.append(_render_tool_action_row(block))
    return (
        '<table class="explore-actions"><thead><tr>'
        "<th>Type</th><th>Path / command</th><th>Size</th>"
        f"</tr></thead><tbody>{''.join(rows)}</tbody></table>"
    )


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

    lines, start, end, marker = _read_file_lines(
        paths.project_root(), rel_path, start, end
    )
    body = f"<pre>{_esc(lines)}</pre>"
    if marker:
        body += f'<small class="trunc-marker">{_esc(marker)}</small>'
    return f'<details><summary>{_esc(range_str)}</summary>{body}</details>'


def _render_explore_status(task_id: str) -> str:
    """Render the Explore section content (the polling wrapper is `#explore-content`).

    Rendered entirely from the stored explore.json, so a page reload restores the last
    run (turns table, summary, refs) without any new pi traffic. When prompts changed
    after the last completed run, a stale banner is shown above the old results."""
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

    polling_attrs = (
        ' hx-trigger="every 1.5s" hx-get="/tasks/{id}/explore-status" hx-swap="outerHTML"'.format(
            id=safe_id
        )
        if status == "running"
        else ""
    )

    parts = [
        f'<div id="explore-content" class="explore-content"{polling_attrs}>'
    ]

    if status == "running":
        parts.append(
            f'<p aria-busy="true">Exploring… hop {state.get("hops_completed", 0) + 1}/{EXPLORE_MAX_HOPS}'
            f" · turns {len(turns)}/{PI_EXPLORE_MAX_TURNS}</p>"
        )
    elif status == "error":
        parts.append(f'<p style="color: #c44;">Error: {_esc(error)}</p>')
    elif status == "done" and explored_at:
        found = state.get("found_files", len(path_refs))
        meta = f"explored {_format_ago(explored_at)} · {len(turns)} turns · {found} files"
        if stop_reason:
            meta += f" · stop: {_esc(str(stop_reason))}"
        parts.append(f'<p class="explore-meta"><small>{meta}</small></p>')
        if paths.prompts_last_modified(task_id) > explored_at:
            parts.append(
                '<p class="explore-stale">Prompts changed since last exploration — Re-explore?</p>'
            )

    if questions:
        items = "".join(f"<li>{_esc(q)}</li>" for q in questions)
        parts.append(
            f'<details class="explore-questions"><summary>Questions ({len(questions)})</summary>'
            f"<ul>{items}</ul></details>"
        )

    if turns:
        parts.append(_render_explore_actions(turns))

    if status == "done" and summary_md:
        parts.append(f'<div class="explore-summary">{_render_markdown(summary_md)}</div>')

    if path_refs:
        parts.append('<section class="explore-refs">')
        parts.append("<h3>Referenced files</h3>")
        for ref in path_refs:
            parts.append(_render_path_ref(ref))
        parts.append("</section>")

    # Allow a new run whenever not already running.
    if status != "running":
        label = "Re-explore" if (turns or summary_md) else "Explore"
        parts.append(
            f'<button hx-post="/api/tasks/{safe_id}/explore" '
            f'hx-target="#explore-content" hx-swap="outerHTML">{label}</button>'
        )

    parts.append("</div>")
    return "".join(parts)


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


def _run_explore_hop(task_id: str, hop: int, message: str, start: float) -> str:
    """Run one hop of the Explore agent and stream its JSON events.

    A hop is capped at EXPLORE_TURNS_PER_HOP turn_end events; the pi session is
    persisted under …/explore/sessions/ so the next hop resumes it via the same
    --session-id. Returns the stop reason: "sentinel", "hop_cap", "turn_cap",
    "time_cap", or "agent_end" (pi finished on its own)."""
    sessions_dir = paths.explore_sessions_dir(task_id)
    sessions_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        "pi",
        "--mode",
        "json",
        "-p",
        "--model",
        EXPLORE_MODEL,
        "--tools",
        "bash,read",
        "--session-id",
        f"explore-{task_id}",
        "--session-dir",
        str(sessions_dir),
        message,
    ]

    logger.info("explore hop %d: full prompt:\n%s", hop, message)
    logger.info("+ %s", shlex.join(cmd))

    _wait_for_rate_limit(EXPLORE_MODEL)
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    current_turn: dict = {}
    state = paths.read_explore_state(task_id)
    total_turns = state.get("turns_completed", 0)
    hop_turns = 0
    reason = "agent_end"
    terminated_by_us = False
    stderr_tail: list[str] = []

    try:
        for line in proc.stdout or []:
            line = line.strip()
            if not line:
                continue

            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                logger.warning("explore hop %d: non-JSON line: %s", hop, line[:200])
                stderr_tail.append(line[:200])
                stderr_tail = stderr_tail[-10:]
                continue

            _apply_explore_event_to_turn(event, current_turn)
            etype = event.get("type")

            if etype == "message_end" and EXPLORE_SENTINEL in current_turn.get("text", ""):
                if _turn_has_content(current_turn):
                    _persist_explore_turn(task_id, current_turn, hop)
                logger.info("explore hop %d: sentinel %s received", hop, EXPLORE_SENTINEL)
                reason = "sentinel"
                terminated_by_us = True
                proc.terminate()
                break

            if etype == "turn_end":
                hop_turns += 1
                total_turns += 1
                _persist_explore_turn(task_id, current_turn, hop)
                logger.info(
                    "explore hop %d: completed turn %d/%d (hop), %d/%d (total)",
                    hop, hop_turns, EXPLORE_TURNS_PER_HOP, total_turns, PI_EXPLORE_MAX_TURNS,
                )
                if total_turns >= PI_EXPLORE_MAX_TURNS:
                    reason = "turn_cap"
                    terminated_by_us = True
                    proc.terminate()
                    break
                if hop_turns >= EXPLORE_TURNS_PER_HOP:
                    reason = "hop_cap"
                    terminated_by_us = True
                    proc.terminate()
                    break

            if time.monotonic() - start > PI_EXPLORE_TIMEOUT_SECONDS:
                if _turn_has_content(current_turn) and etype != "turn_end":
                    _persist_explore_turn(task_id, current_turn, hop)
                reason = "time_cap"
                terminated_by_us = True
                proc.terminate()
                break

            if etype in ("agent_end", "agent_settled"):
                break
    finally:
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    elapsed = time.monotonic() - start
    logger.info(
        "explore hop %d: finished reason=%s hop_turns=%d total_elapsed=%.2fs",
        hop, reason, hop_turns, elapsed,
    )

    if not terminated_by_us and proc.returncode not in (0, None):
        tail = "\n".join(stderr_tail)[:500]
        raise RuntimeError(f"pi exited {proc.returncode}: {tail}")

    return reason


def _run_explore_job(task_id: str) -> None:
    """Hopped Explore run: turn-zero questions → sigmap pre-run → ≤3 gated hops → summary.

    State is persisted to explore.json after every step, so polling and page reloads
    see live progress; the final summary and turn-zero text are also written as
    markdown artefacts under …/<task>/explore/."""
    start = time.monotonic()
    try:
        content = paths.read_all_prompts_joined(task_id)
        state = paths.read_explore_state(task_id)
        state["prompt_last_modified_at"] = paths.prompts_last_modified(task_id)
        paths.write_explore_state(task_id, state)

        # --- Turn zero (nano): questions + hallucinated example answers.
        turn_zero_prompt = _load_template("turn_zero").replace("{content}", content)
        turn_zero_text = _run_pi_text(
            turn_zero_prompt,
            log_prefix="explore-turn-zero",
            model=EXPLORE_GATE_MODEL,
            max_input_chars=TITLE_MAX_INPUT_CHARS,
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
            _load_template("explore")
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
            reason = _run_explore_hop(task_id, hop, message, start)
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
            gate_template = _load_template("gate")
            transcript = _fit_nano_transcript(
                gate_template,
                _render_transcript_plaintext(state.get("turns", [])),
                turn_zero_text,
            )
            gate_prompt = gate_template.replace("{questions}", turn_zero_text).replace(
                "{transcript}", transcript
            )
            gate_reply = _run_pi_text(
                gate_prompt,
                log_prefix=f"explore-gate-hop{hop}",
                model=EXPLORE_GATE_MODEL,
                max_input_chars=TITLE_MAX_INPUT_CHARS,
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
        summary_template = _load_template("explore_summary")
        transcript = _fit_nano_transcript(
            summary_template,
            _render_transcript_plaintext(state.get("turns", [])),
            content,
        )
        summary_prompt = summary_template.replace("{content}", content).replace(
            "{transcript}", transcript
        )
        summary_md = _run_pi_text(
            summary_prompt,
            log_prefix="explore-summary",
            model=EXPLORE_SUMMARY_MODEL,
            max_input_chars=TITLE_MAX_INPUT_CHARS,
        )
        paths.write_explore_artefact(task_id, "explore_summary", summary_md)

        state = paths.read_explore_state(task_id)
        state["summary_md"] = summary_md
        state["path_refs"] = _extract_path_refs(_explore_text_for_refs(state))
        state["found_files"] = len(state["path_refs"])
        state["finished_at"] = time.time()
        state["explored_at"] = state["finished_at"]
        state["status"] = "done"
        paths.write_explore_state(task_id, state)
        logger.info(
            "explore: done stop_reason=%s turns=%d found_files=%d",
            stop_reason, len(state.get("turns", [])), state["found_files"],
        )
    except Exception as e:
        logger.exception("explore worker failed for %s", task_id)
        state = paths.read_explore_state(task_id)
        state["status"] = "error"
        state["error"] = str(e)
        state["finished_at"] = time.time()
        paths.write_explore_state(task_id, state)


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


@app.get("/")
def index() -> HTMLResponse:
    root = paths.project_root()
    tasks = paths.list_task_ids(root)

    if tasks:
        cards = "".join(
            _render_task_card(t, paths.read_info(t, root))
            for t in tasks
        )
    else:
        cards = '<p style="color: #666; text-align: center; padding: 2rem;">No tasks yet — create one below.</p>'

    body = f"""
    <header style="margin-bottom: 2rem;">
      <h1>Crack Tasks</h1>
      <p style="color: #666;">Project: {_esc(str(root))}</p>
    </header>

    <form hx-post="/api/tasks" hx-target="#task-list" hx-swap="afterbegin" hx-on::after-request="this.reset()" style="margin-bottom: 2rem;">
      <h2 style="margin-top: 0;">New Task</h2>
      <div style="display: flex; gap: 0.5rem; flex-wrap: wrap; align-items: flex-end;">
        <div style="flex: 1; min-width: 200px;">
          <label>Title <input type="text" name="title" placeholder="My Task Title" required></label>
        </div>
        <button type="submit" class="primary">Create Task</button>
      </div>
    </form>

    <section id="task-list">
      {cards}
    </section>
    """
    return HTMLResponse(_render_base("Crack Tasks", body))


@app.post("/api/tasks")
def api_create_task(title: str = Form(...)) -> HTMLResponse:
    """Create a new task with an auto-generated id (<ms_timestamp>_<slug title>) and
    return the task card HTML fragment (target: #task-list, swap: afterbegin)."""
    task_id = paths.generate_task_id(title)
    try:
        info = paths.create_task(task_id, title)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    return HTMLResponse(_render_task_card(task_id, info))


@app.delete("/api/tasks/{task_id}")
def api_delete_task(task_id: str) -> HTMLResponse:
    """Delete a task directory. Returns an empty fragment so htmx's outerHTML swap
    removes the task card from the DOM."""
    try:
        task_dir = paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    if not task_dir.exists():
        raise HTTPException(status_code=404, detail="not found")

    for item in task_dir.iterdir():
        if item.is_file():
            item.unlink()
        else:
            shutil.rmtree(item)
    task_dir.rmdir()
    return HTMLResponse("")


@app.get("/api/tasks")
def api_tasks() -> dict:
    root = paths.project_root()
    return {"project_root": str(root), "tasks": paths.list_task_ids(root)}


@app.get("/api/tasks/{task_id}/info")
def api_get_task_info(task_id: str) -> dict:
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"task_id": task_id, "info": paths.read_info(task_id)}


@app.put("/api/tasks/{task_id}/info")
def api_update_task_info(task_id: str, title: str = Form(...)) -> HTMLResponse:
    """Update the task title. Returns the slot content (a fresh title input) plus an
    out-of-band h1 swap (targets: #title-slot innerHTML from both the input auto-save
    and the Save form) — the form submits x-www-form-urlencoded, not JSON."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    info = paths.read_info(task_id)
    info["title"] = title
    paths.write_info(task_id, info)
    return HTMLResponse(
        _render_title_input(task_id, title) + _render_title_h1(task_id, title, oob=True)
    )


@app.get("/api/tasks/{task_id}/prompts")
def api_list_prompts(task_id: str) -> dict:
    try:
        prompt_list = paths.list_prompt_files(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"task_id": task_id, "prompts": prompt_list}


@app.get("/api/tasks/{task_id}/prompts/{filename}")
def api_get_prompt(task_id: str, filename: str) -> dict:
    try:
        content = paths.read_prompt(task_id, filename)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail="not found")
    return {"name": paths.validate_prompt_filename(filename), "content": content}


@app.post("/api/tasks/{task_id}/prompts")
def api_create_prompt(task_id: str, name: str = Form(default=""), content: str = Form(...)) -> HTMLResponse:
    """Create a prompt. If name is blank, auto-assign the next available filename
    (prompt.md, prompt2.md ... prompt9.md). Returns the re-rendered prompts section
    (target: #prompt-list, swap: innerHTML) plus an out-of-band title-regen placeholder."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    filename = name.strip()
    if not filename:
        auto_name = paths.next_prompt_filename(task_id)
        if auto_name is None:
            raise HTTPException(status_code=400, detail="No available prompt slot (prompt.md through prompt9.md all exist)")
        filename = auto_name

    try:
        paths.write_prompt(task_id, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    _start_title_regen_job(task_id)
    return HTMLResponse(
        _render_prompts_section(task_id) + _render_title_regen_pending(task_id, oob=True)
    )


@app.put("/api/tasks/{task_id}/prompts/{filename}")
def api_update_prompt(task_id: str, filename: str, content: str = Form(...)) -> HTMLResponse:
    """Save prompt content. Returns the re-rendered read-only row (target: closest
    article, swap: outerHTML) so the row toggles back from editable to non-editable."""
    try:
        old_content = paths.read_prompt(task_id, filename)
    except FileNotFoundError:
        old_content = ""

    try:
        paths.write_prompt(task_id, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    if content != old_content:
        _start_title_regen_job(task_id)
        return HTMLResponse(
            _render_prompt_row(task_id, filename, editing=False)
            + _render_title_regen_pending(task_id, oob=True)
        )

    return HTMLResponse(_render_prompt_row(task_id, filename, editing=False))


@app.delete("/api/tasks/{task_id}/prompts/{filename}")
def api_delete_prompt(task_id: str, filename: str) -> HTMLResponse:
    """Returns an empty fragment so htmx's outerHTML swap removes the row."""
    try:
        paths.delete_prompt(task_id, filename)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail="not found")

    _start_title_regen_job(task_id)
    return HTMLResponse("" + _render_title_regen_pending(task_id, oob=True))


@app.post("/api/tasks/{task_id}/regenerate-title")
def api_regenerate_task_title(task_id: str) -> HTMLResponse:
    """Kick off a background title regeneration and return the polling placeholder."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    _start_title_regen_job(task_id)
    return HTMLResponse(_render_title_regen_pending(task_id))


@app.get("/tasks/{task_id}/title-regen-status", response_class=HTMLResponse)
def title_regen_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background title regeneration. When it first observes a
    'done' state it also writes the new title to info.json (auto-save)."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    state = paths.read_title_regen_state(task_id)
    status = state.get("status")

    if status == "running":
        return HTMLResponse(_render_title_regen_pending(task_id))

    if status == "done":
        title = state.get("title", task_id)
        info = paths.read_info(task_id)
        info["title"] = title
        paths.write_info(task_id, info)
        # Mark the job as saved so future polls return the normal input without re-saving.
        paths.write_title_regen_state(task_id, {"status": "saved", "title": title})
        return HTMLResponse(
            _render_title_input(task_id, title) + _render_title_h1(task_id, title, oob=True)
        )

    if status == "error":
        return HTMLResponse(_render_title_regen_error(task_id, state.get("error", "unknown error")))

    # saved, idle, or missing state — just render the current title input.
    info = paths.read_info(task_id)
    return HTMLResponse(_render_title_input(task_id, info.get("title", task_id)))


@app.post("/api/tasks/{task_id}/explore")
def api_explore(task_id: str) -> HTMLResponse:
    """Start a background Explore run, or return the current status if one is running."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    state = paths.read_explore_state(task_id)
    if state.get("status") != "running":
        _start_explore_job(task_id)
    return HTMLResponse(_render_explore_status(task_id))


@app.get("/tasks/{task_id}/explore-status", response_class=HTMLResponse)
def explore_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background Explore run."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(_render_explore_status(task_id))


@app.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> HTMLResponse:
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    info = paths.read_info(task_id)
    safe_id = _esc(task_id)
    safe_title = _esc(info.get("title", task_id))
    header = _render_task_header(task_id, info)
    next_name = paths.next_prompt_filename(task_id) or "prompt.md"

    body = f"""
    {header}
    <section id="prompt-list">
      <div hx-get="/tasks/{safe_id}/prompts-list" hx-trigger="load"></div>
    </section>

    <details class="add">
      <summary style="font-size: 0.95rem; cursor: pointer;">Add Prompt</summary>
      <form hx-post="/api/tasks/{safe_id}/prompts" hx-target="#prompt-list" hx-swap="innerHTML" hx-on::after-request="this.reset()">
        <label>Filename (optional) <input type="text" name="name" placeholder="blank = {_esc(next_name)}" pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]*\\.md"></label>
        <label>Content <textarea name="content" rows="4" placeholder="Markdown content…" required></textarea></label>
        <button type="submit">Add Prompt</button>
      </form>
    </details>

    <section class="explore" id="explore-section">
      <h2>Explore</h2>
      {_render_explore_status(task_id)}
    </section>
    """
    return HTMLResponse(_render_base(f"Crack Task: {safe_title}", body, task_id))


@app.get("/tasks/{task_id}/prompts-list", response_class=HTMLResponse)
def task_prompts_list(task_id: str) -> HTMLResponse:
    """Return the prompt list HTML fragment for htmx (initial load on the task page)."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(_render_prompts_section(task_id))


@app.get("/tasks/{task_id}/prompt-row/{filename}", response_class=HTMLResponse)
def prompt_row(task_id: str, filename: str, editing: bool = Query(default=False)) -> HTMLResponse:
    """Return one prompt row in view or edit mode (target: closest article, swap:
    outerHTML) — this is how Edit/Cancel toggle a row in place without a separate panel."""
    try:
        return HTMLResponse(_render_prompt_row(task_id, filename, editing=editing))
    except FileNotFoundError as e:
        raise HTTPException(status_code=404, detail="not found") from e
