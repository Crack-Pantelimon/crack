"""Faithful projection of pi session ``*.jsonl`` into renderable view rows.

The UI renders from this projection (not the server-rebuilt ``exchanges[].turns``
store). Every session event becomes a row; unrecognised types get a type-labelled
Expand control revealing the raw JSON. Parse results are cached by
``(path, size, mtime)`` so the 2 s poll does not re-parse unchanged files.
"""

from __future__ import annotations

import json
import logging
import time
from datetime import datetime
from pathlib import Path
from typing import Any

from crack_server.steprun import attach_media_to_blocks
from crack_server.transcript import apply_event_to_turn, text_from_content, turn_has_content

logger = logging.getLogger("uvicorn.error")


def _row_epoch(row: dict) -> float | None:
    """Best-effort epoch for a trajectory row: prefer a harness ``at`` float, else
    parse a session event's ISO ``timestamp`` (…Z). None when neither is present."""
    at = row.get("at")
    if at is not None:
        try:
            return float(at)
        except (TypeError, ValueError):
            pass
    ts = row.get("timestamp")
    if ts:
        try:
            return datetime.fromisoformat(str(ts).replace("Z", "+00:00")).timestamp()
        except ValueError:
            pass
    return None


# Cache: path → (mtime_ns, size, parsed events list)
_FILE_CACHE: dict[str, tuple[int, int, list[dict]]] = {}


def _stat_key(path: Path) -> tuple[int, int] | None:
    try:
        st = path.stat()
    except OSError:
        return None
    return (getattr(st, "st_mtime_ns", int(st.st_mtime * 1e9)), st.st_size)


def _read_session_events(path: Path) -> list[dict]:
    """Parse one session ndjson file; cached by size+mtime."""
    key = _stat_key(path)
    cache_key = str(path)
    if key is not None:
        hit = _FILE_CACHE.get(cache_key)
        if hit and hit[0] == key[0] and hit[1] == key[1]:
            return hit[2]
    events: list[dict] = []
    try:
        raw = path.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        logger.warning("trajectory: cannot read %s: %s", path, e)
        return []
    for line in raw.splitlines():
        line = line.strip()
        if not line or line.count("\x00") > len(line) // 2:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            events.append({
                "type": "_unparseable",
                "id": f"bad-{len(events)}",
                "raw": line[:2000],
            })
            continue
        if isinstance(obj, dict):
            events.append(obj)
    if key is not None:
        _FILE_CACHE[cache_key] = (key[0], key[1], events)
    return events


def list_session_files(sessions_dir: Path) -> list[Path]:
    """Session ndjson files in chronological order (filename timestamp)."""
    if not sessions_dir.is_dir():
        return []
    return sorted(
        p for p in sessions_dir.iterdir()
        if p.is_file() and p.suffix == ".jsonl"
    )


def _flush_turn(turn: dict, event_id: str, rows: list[dict]) -> dict:
    """Append a completed assistant turn row; return a fresh accumulator."""
    if turn_has_content(turn):
        row = {
            "kind": "turn",
            "id": event_id or f"turn-{len(rows)}",
            "text": turn.get("text", ""),
            "thinking": turn.get("thinking", ""),
            "tool_blocks": list(turn.get("tool_blocks") or []),
            "model": turn.get("model") or "",
            "timestamp": turn.get("timestamp"),
        }
        rows.append(row)
    return {"text": "", "thinking": "", "tool_blocks": []}


def project_session_events(events: list[dict]) -> list[dict]:
    """Project raw pi session events into ordered view rows."""
    rows: list[dict] = []
    current: dict = {"text": "", "thinking": "", "tool_blocks": []}
    current_id = ""
    current_model = ""

    for event in events:
        etype = event.get("type")
        eid = str(event.get("id") or "")
        ts = event.get("timestamp")

        if etype == "session":
            rows.append({
                "kind": "annotation",
                "ann": "session",
                "id": eid or f"session-{len(rows)}",
                "label": f"session {event.get('id') or ''}".strip(),
                "timestamp": ts,
                "raw": event,
            })
            continue

        if etype == "model_change":
            model = str(event.get("modelId") or event.get("model") or "")
            provider = str(event.get("provider") or "")
            current_model = f"{provider}/{model}" if provider and model and "/" not in model else (model or provider)
            rows.append({
                "kind": "annotation",
                "ann": "model_change",
                "id": eid or f"model-{len(rows)}",
                "label": f"model → {current_model}",
                "model": current_model,
                "timestamp": ts,
                "raw": event,
            })
            continue

        if etype == "thinking_level_change":
            level = event.get("thinkingLevel") or event.get("level") or "?"
            rows.append({
                "kind": "annotation",
                "ann": "thinking_level_change",
                "id": eid or f"think-{len(rows)}",
                "label": f"thinking level → {level}",
                "timestamp": ts,
                "raw": event,
            })
            continue

        if etype == "message":
            message = event.get("message")
            if not isinstance(message, dict):
                rows.append({
                    "kind": "unknown",
                    "id": eid or f"unk-{len(rows)}",
                    "label": "message",
                    "timestamp": ts,
                    "raw": event,
                })
                continue
            role = message.get("role")
            if role == "user":
                # Flush any open assistant turn before a user message.
                current = _flush_turn(current, current_id, rows)
                text = text_from_content(message.get("content"))
                rows.append({
                    "kind": "session_user",
                    "id": eid or f"user-{len(rows)}",
                    "text": text,
                    "timestamp": ts,
                    "raw": event,
                })
                continue
            if role == "toolResult":
                # Merge into the open turn via the same helper the stream uses.
                apply_event_to_turn(
                    {"type": "message_end", "message": message},
                    current,
                )
                continue
            if role == "assistant":
                # Previous assistant message (if any) is complete — flush it.
                current = _flush_turn(current, current_id, rows)
                current_id = eid
                apply_event_to_turn({"type": "turn_start"}, current)
                apply_event_to_turn(
                    {"type": "message_end", "message": message},
                    current,
                )
                if current_model:
                    current["model"] = current_model
                current["timestamp"] = ts
                # Do not flush yet — toolResults may follow.
                continue
            if role == "error":
                current = _flush_turn(current, current_id, rows)
                rows.append({
                    "kind": "unknown",
                    "id": eid or f"err-{len(rows)}",
                    "label": "error",
                    "timestamp": ts,
                    "raw": event,
                })
                continue
            rows.append({
                "kind": "unknown",
                "id": eid or f"unk-{len(rows)}",
                "label": f"message:{role}",
                "timestamp": ts,
                "raw": event,
            })
            continue

        # Anything else (custom, agent_end, …) — faithful unknown row.
        label = str(etype or "event")
        if etype == "custom" and event.get("customType"):
            label = f"custom:{event.get('customType')}"
        rows.append({
            "kind": "unknown",
            "id": eid or f"unk-{len(rows)}",
            "label": label,
            "timestamp": ts,
            "raw": event,
        })

    _flush_turn(current, current_id, rows)
    return rows


def project_sessions_dir(
    sessions_dir: Path,
    *,
    media_dir: Path | None = None,
    media_url_prefix: str = "",
) -> list[dict]:
    """Read all session files under ``sessions_dir`` and project to view rows."""
    rows: list[dict] = []
    for path in list_session_files(sessions_dir):
        events = _read_session_events(path)
        part = project_session_events(events)
        if media_dir is not None:
            for row in part:
                if row.get("kind") == "turn" and row.get("tool_blocks"):
                    row["tool_blocks"] = attach_media_to_blocks(
                        row["tool_blocks"], media_dir, media_url_prefix,
                    )
        rows.extend(part)
    return rows


def merge_exchange_sidecars(
    projected: list[dict],
    exchanges: list[dict],
) -> list[dict]:
    """Merge harness-only constructs into the projected stream.

    The session ndjson is the spine (including user messages). Exchange sidecars
    supply ask_user Q&A, recorded errors, and richer user-prompt metadata
    (compiled prompt / media) that pi's session file does not carry.
    """
    # Map stripped user text → exchange sidecar metadata.
    prompt_meta: dict[str, dict] = {}
    error_rows: list[dict] = []
    qa_rows: list[dict] = []
    terminal_rows: list[dict] = []
    for i, exchange in enumerate(exchanges):
        prompt_entry = next(
            (t for t in (exchange.get("turns") or []) if t.get("kind") == "user_prompt"),
            None,
        )
        qa = exchange.get("qa")
        if qa:
            # Time the Q&A card so it sorts into the stream by timestamp rather
            # than being dumped at the top. Prefer a recorded qa["at"] (the answer
            # moment); fall back to the exchange's user_prompt time, which is when
            # the answered prompt was compiled — the same instant, near enough.
            at = qa.get("at")
            if at is None and prompt_entry is not None:
                at = prompt_entry.get("at")
            qa_rows.append({"kind": "ask_user_qa", "id": f"qa-{i}", "qa": qa, "at": at})
        user_text = str(exchange.get("user") or "").strip()
        media = exchange.get("media") or []
        if user_text:
            meta: dict[str, Any] = {
                "original": user_text,
                "label": "chat",
            }
            if prompt_entry is not None:
                meta["compiled"] = prompt_entry.get("compiled") or ""
                meta["template"] = prompt_entry.get("template") or ""
            if media:
                meta["media"] = media
            prompt_meta[user_text] = meta
        for j, err in enumerate(exchange.get("errors") or []):
            e = dict(err)
            e["kind"] = "error"
            e["id"] = f"err-{i}-{j}"
            error_rows.append(e)
        stop_reason = exchange.get("stop_reason")
        if stop_reason:
            # Place after the exchange's last turn (or at "now" when stopped
            # before any trajectory landed).
            at = None
            for t in reversed(exchange.get("turns") or []):
                if t.get("at") is not None:
                    at = t["at"]
                    break
            if at is None:
                at = time.time()
            terminal_rows.append({
                "kind": "terminal_reason",
                "id": f"stop-{i}",
                "reason": stop_reason,
                "at": at,
            })

    out: list[dict] = []
    # The session ndjson is the sole source of user-prompt rows: a prompt shows
    # up only once pi records it (as a `session_user` message), enriched with the
    # exchange sidecar's compiled-prompt / media metadata. We deliberately do NOT
    # synthesize a prompt row for exchanges not yet in the session file — an
    # optimistic echo lands at a different index than the eventual trajectory row
    # and the append-by-index poll then duplicates it.
    for row in projected:
        if row.get("kind") == "session_user":
            text = str(row.get("text") or "").strip()
            meta = prompt_meta.get(text) or {
                "original": text,
                "label": "chat",
                "compiled": "",
            }
            out.append({
                "kind": "user_prompt",
                "id": row.get("id") or f"prompt-{len(out)}",
                **meta,
                "timestamp": row.get("timestamp"),
            })
            continue
        out.append(row)

    # Merge sidecar rows (errors, ask_user Q&A) into the projected stream by time
    # instead of dumping them at the top/end. `out` rows carry a monotonic
    # (carry-forward) epoch; sidecars sort by their own `at`, and on ties land
    # after the spine row they follow.
    keyed: list[tuple[float, int, int, dict]] = []
    last = 0.0
    for idx, row in enumerate(out):
        ep = _row_epoch(row)
        if ep is None or ep < last:
            ep = last            # carry forward so out order is preserved
        else:
            last = ep
        keyed.append((ep, 0, idx, row))
    for idx, side in enumerate(error_rows + qa_rows + terminal_rows):
        ep = _row_epoch(side)
        keyed.append((ep if ep is not None else last, 1, idx, side))
    keyed.sort(key=lambda item: (item[0], item[1], item[2]))
    return [payload for _, _, _, payload in keyed]


def clear_cache() -> None:
    """Test helper: drop the session-file parse cache."""
    _FILE_CACHE.clear()
