"""Rolling summarizer compaction: shrink pi session context at 75% window."""

from __future__ import annotations

import json
import logging
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

from crack_server import models as models_mod
from crack_server import paths, pi_runner
from crack_server.context_stats import session_usage
from crack_server.pi_proc import arun_pi_text
from crack_server.state import JsonState
from crack_server.transcript import text_from_content
from crack_server.trajectory_view import _read_session_events, list_session_files

logger = logging.getLogger("uvicorn.error")

COMPACTION_THRESHOLD = 0.75
RETAIN_TOKENS = 20_000
CHARS_PER_TOKEN = 4

SUMMARY_PROMPT = """You are summarizing a coding-agent conversation for context compaction.
Preserve facts the agent will need to continue: goals, files touched, decisions, errors, and open work.

Write a structured summary with exactly these five markdown headings:

# Goal
# Progress
# Key decisions
# Current state
# Open items

Be concise but complete. Use bullet lists under each heading where helpful.

Transcript to summarize:
"""


def should_compact(sessions_dir: Path, model: str) -> bool:
    """Return True when session usage is at or above the compaction threshold."""
    window = models_mod.context_window(model)
    usage = session_usage(sessions_dir)
    if usage is None:
        return False
    tokens = int(usage.get("tokens") or 0)
    if tokens <= 0:
        return False
    return tokens / float(window) >= COMPACTION_THRESHOLD


def resolve_session_id(state: dict, base_session_id: str) -> str:
    """Active pi session id (compacted suffix when present)."""
    return str(state.get("pi_session_id") or base_session_id)


def _active_session_path(sessions_dir: Path) -> Path | None:
    files = list_session_files(sessions_dir)
    return files[-1] if files else None


def _estimate_event_tokens(event: dict) -> int:
    """Rough token estimate for one session ndjson event."""
    try:
        raw = json.dumps(event, ensure_ascii=False)
    except (TypeError, ValueError):
        raw = str(event)
    return max(1, len(raw) // CHARS_PER_TOKEN)


def _message_count(events: list[dict]) -> int:
    count = 0
    for event in events:
        if event.get("type") != "message":
            continue
        message = event.get("message")
        if not isinstance(message, dict):
            continue
        role = message.get("role")
        if role in ("user", "assistant"):
            count += 1
    return count


def _is_tool_result(event: dict) -> bool:
    if event.get("type") != "message":
        return False
    message = event.get("message")
    return isinstance(message, dict) and message.get("role") == "toolResult"


def _assistant_has_tool_calls(event: dict) -> bool:
    if event.get("type") != "message":
        return False
    message = event.get("message")
    if not isinstance(message, dict) or message.get("role") != "assistant":
        return False
    content = message.get("content")
    if not isinstance(content, list):
        return False
    return any(isinstance(part, dict) and part.get("type") == "toolCall" for part in content)


def _group_start(events: list[dict], idx: int) -> int:
    if idx < 0 or idx >= len(events):
        return max(0, idx)
    if not _is_tool_result(events[idx]):
        return idx
    j = idx - 1
    while j >= 0:
        if _assistant_has_tool_calls(events[j]):
            return j
        if _is_tool_result(events[j]):
            j -= 1
            continue
        break
    return idx


def _group_end(events: list[dict], start: int) -> int:
    if start < 0 or start >= len(events) or not _assistant_has_tool_calls(events[start]):
        return start + 1
    j = start + 1
    while j < len(events) and _is_tool_result(events[j]):
        j += 1
    return j


def _align_cutoff(events: list[dict], cut: int) -> int:
    n = len(events)
    if cut <= 0 or cut >= n:
        return max(0, min(cut, n))
    if _is_tool_result(events[cut]):
        return _group_start(events, cut)
    if cut > 0 and _assistant_has_tool_calls(events[cut - 1]):
        group_end = _group_end(events, cut - 1)
        if cut < group_end:
            return cut - 1
    return cut


def _find_cutoff_index(events: list[dict], retain_tokens: int = RETAIN_TOKENS) -> int:
    """Index splitting prefix (summarized) from tail (retained), tool-group safe."""
    if not events:
        return 0
    estimates = [_estimate_event_tokens(e) for e in events]
    tail_tokens = 0
    cut = len(events)
    for i in range(len(events) - 1, -1, -1):
        tail_tokens += estimates[i]
        cut = i
        if tail_tokens >= retain_tokens:
            break
    return _align_cutoff(events, cut)


def _events_transcript(events: list[dict]) -> str:
    """Plain-text transcript for prefix events."""
    lines: list[str] = []
    for event in events:
        etype = event.get("type")
        if etype == "session":
            lines.append(f"[session {event.get('id', '')}]")
            continue
        if etype == "model_change":
            provider = event.get("provider") or ""
            model = event.get("modelId") or event.get("model") or ""
            lines.append(f"[model → {provider}/{model}]".rstrip("/"))
            continue
        if etype != "message":
            lines.append(f"[{etype or 'event'}]")
            continue
        message = event.get("message")
        if not isinstance(message, dict):
            continue
        role = message.get("role") or "?"
        text = text_from_content(message.get("content"))
        if role == "toolResult":
            name = message.get("toolName") or "tool"
            lines.append(f"toolResult ({name}): {text[:2000]}")
        else:
            lines.append(f"{role}: {text[:4000]}")
    return "\n".join(lines)


def _fallback_summary(transcript: str, events: list[dict]) -> str:
    """Deterministic local summary when the LLM call fails."""
    n_events = len(events)
    n_msgs = _message_count(events)
    preview = transcript.strip()
    if len(preview) > 4000:
        preview = preview[:2000] + "\n…\n" + preview[-1500:]
    return (
        "# Goal\n"
        "- Continue the in-progress coding task (auto-generated compaction summary).\n\n"
        "# Progress\n"
        f"- Compacted {n_events} session events ({n_msgs} user/assistant messages).\n"
        f"- Recent transcript excerpt preserved below.\n\n"
        "# Key decisions\n"
        "- (unavailable — LLM summarizer failed; see excerpt)\n\n"
        "# Current state\n"
        f"{preview or '(empty transcript)'}\n\n"
        "# Open items\n"
        "- Resume from the retained tail of the conversation.\n"
    )


async def generate_summary(transcript: str, model: str) -> tuple[str, str]:
    """Return (summary_text, source) where source is ``llm`` or ``fallback``."""
    prompt = SUMMARY_PROMPT + transcript
    try:
        text, _elapsed = await arun_pi_text(
            prompt,
            log_prefix="compaction/summary",
            model=model,
        )
        cleaned = (text or "").strip()
        if cleaned:
            return cleaned, "llm"
    except Exception:
        logger.exception("compaction: LLM summary failed for model=%s", model)
    return _fallback_summary(transcript, []), "fallback"


def _session_event(session_id: str) -> dict:
    return {
        "type": "session",
        "id": session_id,
        "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }


def _summary_user_event(summary: str) -> dict:
    return {
        "type": "message",
        "id": f"compaction-{uuid.uuid4().hex[:12]}",
        "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "message": {
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": (
                        "[Context compaction summary — earlier messages were summarized "
                        "to free context window]\n\n"
                        f"{summary}"
                    ),
                }
            ],
        },
    }


def _session_filename(session_id: str) -> str:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ")
    return f"{ts}_{session_id}.jsonl"


def seed_compacted_session(
    sessions_dir: Path,
    session_id: str,
    summary: str,
    retained_events: list[dict],
) -> Path:
    """Write a new immutable session file; return its path."""
    sessions_dir.mkdir(parents=True, exist_ok=True)
    path = sessions_dir / _session_filename(session_id)
    lines = [
        json.dumps(_session_event(session_id), ensure_ascii=False),
        json.dumps(_summary_user_event(summary), ensure_ascii=False),
    ]
    for event in retained_events:
        lines.append(json.dumps(event, ensure_ascii=False))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def _usage_tokens(sessions_dir: Path) -> int:
    usage = session_usage(sessions_dir)
    if usage is None:
        return 0
    return int(usage.get("tokens") or 0)


async def compact_if_needed(
    *,
    state_obj: JsonState,
    sessions_dir: Path,
    model: str,
    base_session_id: str,
    pid_file: Path | None = None,
    log_prefix: str,
) -> str:
    """Compact when over threshold; return the active session id for the next hop."""
    state = state_obj.read()
    active_id = resolve_session_id(state, base_session_id)
    if not should_compact(sessions_dir, model):
        return active_id

    started = time.monotonic()
    session_path = _active_session_path(sessions_dir)
    events = _read_session_events(session_path) if session_path else []
    tokens_before = _usage_tokens(sessions_dir)
    messages_before = _message_count(events)

    try:
        if pid_file is not None:
            pi_runner.kill_pid_file(pid_file)

        cut = _find_cutoff_index(events)
        prefix = events[:cut]
        retained = events[cut:]
        if not prefix and len(events) > 1:
            # Meter is full but char-based estimates under-count (e.g. driver usage
            # fields). Summarize an older fraction so compaction still helps.
            cut = max(1, int(len(events) * 0.75))
            prefix = events[:cut]
            retained = events[cut:]
        if not prefix:
            logger.info("%s: compaction skipped — nothing to summarize", log_prefix)
            return active_id

        transcript = _events_transcript(prefix)
        summary, source = await generate_summary(transcript, model)
        if source == "fallback":
            summary = _fallback_summary(transcript, prefix)

        count = int(state.get("compaction_count") or 0) + 1
        new_session_id = f"{base_session_id}-c{count}"
        seed_compacted_session(sessions_dir, new_session_id, summary, retained)

        from crack_server import trajectory_view

        trajectory_view.clear_cache()
        # Drop cached usage for the superseded session file.
        if session_path is not None:
            from crack_server.context_stats import _USAGE_CACHE

            _USAGE_CACHE.pop(str(session_path), None)

        def _upd(s: dict) -> dict:
            s["pi_session_id"] = new_session_id
            s["compaction_count"] = count
            return s

        state_obj.update(_upd)

        tokens_after = sum(_estimate_event_tokens(e) for e in retained)
        messages_after = _message_count(retained)
        duration_s = round(time.monotonic() - started, 2)
        paths.append_traj_note(
            state_obj,
            "compaction",
            f"Context compacted ({source})",
            status="ok",
            tokens_before=tokens_before,
            tokens_after=tokens_after,
            messages_before=messages_before,
            messages_after=messages_after,
            duration_s=duration_s,
            detail=f"session {active_id} → {new_session_id}",
        )
        logger.info(
            "%s: compacted session %s → %s (tokens %d→%d, msgs %d→%d, %.2fs)",
            log_prefix,
            active_id,
            new_session_id,
            tokens_before,
            tokens_after,
            messages_before,
            messages_after,
            duration_s,
        )
        return new_session_id
    except Exception as exc:
        duration_s = round(time.monotonic() - started, 2)
        logger.exception("%s: compaction failed", log_prefix)
        paths.append_traj_note(
            state_obj,
            "compaction",
            "Context compaction failed",
            status="err",
            tokens_before=tokens_before,
            tokens_after=tokens_before,
            messages_before=messages_before,
            messages_after=messages_before,
            duration_s=duration_s,
            detail=str(exc),
        )
        return active_id
