"""Turn accumulation, transcript rendering, and path-ref extraction.

Split out of pi_runner.py (A6): pure helpers that turn pi's JSON events into
turn dicts and render/clamp transcripts for gate/summary prompts, plus the
file-reference extraction used by the Explore stage.
"""

from __future__ import annotations

import re
from pathlib import Path

from crack_server import paths
from crack_server.ratelimit import TITLE_MAX_INPUT_CHARS

READ_MAX_LINES = 200
READ_MAX_CHARS = 10_000

_PATH_REF_RE = re.compile(
    r"`?([A-Za-z0-9_][A-Za-z0-9_./-]*\.[A-Za-z]{1,10})`?(?::(\d+)(?:-(\d+))?)?"
)


def text_from_content(content) -> str:
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


def apply_event_to_turn(event: dict, current_turn: dict) -> None:
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
        output = text_from_content(content)
        tool_call_id = message.get("toolCallId")
        # Merge the result into the matching toolCall block, if present.
        merged = False
        is_error = bool(message.get("isError"))
        for block in current_turn.get("tool_blocks", []):
            if block.get("id") == tool_call_id:
                block["output"] = output
                block["is_error"] = is_error
                merged = True
                break
        if not merged:
            current_turn.setdefault("tool_blocks", []).append(
                {
                    "id": tool_call_id,
                    "name": message.get("toolName", "tool"),
                    "input": "",
                    "output": output,
                    "is_error": is_error,
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


def turn_has_content(current_turn: dict) -> bool:
    return bool(
        current_turn.get("text", "").strip()
        or current_turn.get("thinking", "").strip()
        or current_turn.get("tool_blocks")
    )


def count_turn_groups(turns: list[dict]) -> int:
    """Count turn *groups* for display: a consecutive streak of tool-calling
    turns counts once, since a run of turns that only exist to drive tools is
    one unit of model reasoning. Prompt entries (dicts carrying a ``kind`` key,
    e.g. recorded user prompts) are skipped — they are not turns."""
    count = 0
    prev_had_tools = False
    for turn in turns:
        if turn.get("kind"):
            continue
        had_tools = bool(turn.get("tool_blocks"))
        if not (had_tools and prev_had_tools):
            count += 1
        prev_had_tools = had_tools
    return count


def truncate_output(text: str, max_lines: int = READ_MAX_LINES, max_chars: int = READ_MAX_CHARS) -> tuple[str, str | None]:
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


def tail_truncate(text: str, max_chars: int) -> str:
    """Keep the tail of a long transcript (recent turns matter most to gate/summary)."""
    if len(text) <= max_chars:
        return text
    return "… [earlier transcript omitted]\n" + text[-max_chars:]


def fit_nano_transcript(template: str, transcript: str, *other_parts: str) -> str:
    """Tail-truncate a transcript so template + other parts + transcript fit the nano
    input limit. The hard cut in `run_pi_text` would otherwise chop the tail — the
    most recent, most useful turns."""
    used = len(template) + sum(len(p) for p in other_parts) + 200  # safety margin
    return tail_truncate(transcript, max(2_000, TITLE_MAX_INPUT_CHARS - used))


def render_transcript_plaintext(turns: list[dict]) -> str:
    """Render a plaintext transcript of agent turns for gate/summary prompts.

    Prompt entries (``kind`` key) are skipped — the transcript shows the
    agent's work, not the harness's own prompts."""
    parts = []
    i = 0
    for turn in turns:
        if turn.get("kind"):
            continue
        i += 1
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


def resolve_path_ref(root: Path, candidate: str) -> Path | None:
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


def extract_path_refs(text: str) -> list[dict]:
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

        abs_path = resolve_path_ref(root, candidate)
        if abs_path is None:
            continue
        rel_path = abs_path.relative_to(root).as_posix()

        key = (rel_path, start, end)
        if key in seen:
            continue
        seen.add(key)

        refs.append({"rel_path": rel_path, "start": start, "end": end})

    return refs


def read_file_lines(root: Path, rel_path: str, start: int | None, end: int | None) -> tuple[str, int, int, str | None]:
    """Read a clamped line range from a project file.

    Returns (text, start, end, truncation_marker). The range is capped at
    READ_MAX_LINES lines and the text at READ_MAX_CHARS chars."""
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
    if end - start + 1 > READ_MAX_LINES:
        end = start + READ_MAX_LINES - 1

    text, marker = truncate_output("\n".join(lines[start - 1 : end]))
    return text, start, end, marker
