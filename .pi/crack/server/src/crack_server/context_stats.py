"""Context-window usage for the chat/sub-agent status lines (pi-CLI style).

The hop stream (``agent.hop.jsonl``) carries no token usage, but the pi
**session** jsonl does: every assistant message holds
``usage:{input, cacheRead, output, totalTokens, cost}``. The last such
message's ``input + cacheRead`` is the prompt size that produced it — i.e. how
full the context currently is. Combined with the model's context window (from
the enriched models cache, see :mod:`models`) that yields the percent meter.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path

from crack_server import models as models_mod
from crack_server import ui as _ui

logger = logging.getLogger("uvicorn.error")

# Only read the tail of a session file — the last assistant message with usage
# is always near the end, and session files can grow large.
_TAIL_BYTES = 512 * 1024


def _newest_session(sessions_dir: Path) -> Path | None:
    if not sessions_dir.is_dir():
        return None
    files = sorted(sessions_dir.glob("*.jsonl"), key=lambda p: p.stat().st_mtime)
    return files[-1] if files else None


def _read_tail_lines(path: Path) -> list[str]:
    try:
        size = path.stat().st_size
        with path.open("rb") as fh:
            if size > _TAIL_BYTES:
                fh.seek(size - _TAIL_BYTES)
                fh.readline()  # drop the partial first line
            data = fh.read()
    except OSError:
        return []
    return data.decode("utf-8", "replace").splitlines()


def session_usage(sessions_dir: Path) -> dict | None:
    """Latest context usage for a chat/run's pi session, or None.

    Returns ``{tokens, output, total, cost, model}`` where ``tokens`` is the
    context tokens consumed (``input + cacheRead``) by the most recent assistant
    message that reported non-zero usage."""
    session = _newest_session(sessions_dir)
    if session is None:
        return None
    for line in reversed(_read_tail_lines(session)):
        line = line.strip()
        if not line or '"usage"' not in line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        message = obj.get("message") if isinstance(obj, dict) else None
        if not isinstance(message, dict) or message.get("role") != "assistant":
            continue
        usage = message.get("usage")
        if not isinstance(usage, dict):
            continue
        tokens = int(usage.get("input", 0) or 0) + int(usage.get("cacheRead", 0) or 0)
        if tokens <= 0:
            continue
        cost = usage.get("cost") or {}
        return {
            "tokens": tokens,
            "output": int(usage.get("output", 0) or 0),
            "total": int(usage.get("totalTokens", 0) or 0),
            "cost": float(cost.get("total", 0) or 0) if isinstance(cost, dict) else 0.0,
            "model": str(message.get("model") or ""),
        }
    return None


def _fmt_tokens(n: int) -> str:
    if n >= 1_000_000:
        return f"{n / 1_000_000:.2f}M"
    if n >= 1_000:
        return f"{n / 1_000:.1f}k"
    return str(n)


def render_context_line(sessions_dir: Path, fallback_model: str = "") -> str:
    """A pinned status line: ``used / window (pct%)`` meter + cost.

    Empty string when no usage has been recorded yet (e.g. a brand-new chat)."""
    usage = session_usage(sessions_dir)
    if usage is None:
        return ""
    esc = _ui._esc
    model = usage["model"] or fallback_model
    window = models_mod.context_window(model) if model else None
    tokens = usage["tokens"]
    meter = ""
    label = f"{_fmt_tokens(tokens)} tok"
    if window and window > 0:
        pct = min(100.0, tokens * 100.0 / window)
        label = f"{_fmt_tokens(tokens)} / {_fmt_tokens(window)} tok · {pct:.0f}%"
        meter = (
            f'<span class="ctx-meter"><span class="ctx-meter-fill" '
            f'style="width:{pct:.1f}%"></span></span>'
        )
    cost = usage["cost"]
    cost_str = f" · ${cost:.4f}" if cost else ""
    model_str = f'<code>{esc(model)}</code> ' if model else ""
    return (
        f'<div class="ctx-line">{model_str}{meter}'
        f'<span class="ctx-label">{esc(label)}{esc(cost_str)}</span></div>'
    )
