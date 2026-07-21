"""Shared turn-persistence and chat error helpers.

Extracted from the deleted stages package for chats and sub-agents.
"""

from __future__ import annotations

import json
import logging
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Callable, Iterator

from crack_server.ratelimit import MAX_TOTAL_ERRORS
from crack_server.state import JsonState

logger = logging.getLogger("uvicorn.error")

def attach_media_to_blocks(
    tool_blocks: list[dict], media_dir: Path, url_prefix: str
) -> list[dict]:
    """Copy images referenced by read/analyze_image tool calls into ``media_dir``
    and attach a ``media: [{src, url}]`` field to their blocks.

    Candidates: ``read`` calls whose path has an image extension, and every
    path in an ``analyze_image`` call's ``image_paths``. Missing/corrupt/
    non-image files are skipped silently (save_validated_copy returns None).
    The saved copy persists inside the task/chat/run dir, so thumbnails keep
    working even if the source path is later deleted.
    """
    from crack_server import images as images_mod
    from crack_server.paths import project_root

    out: list[dict] = []
    for block in tool_blocks:
        block = dict(block)
        name = str(block.get("name", ""))
        args = block.get("input")
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except json.JSONDecodeError:
                args = {}
        if not isinstance(args, dict):
            args = {}

        candidates: list[str] = []
        if name == "read":
            raw = str(args.get("path") or "")
            if raw and Path(raw).suffix.lower() in images_mod.IMAGE_EXTS:
                candidates.append(raw)
        elif name == "analyze_image":
            raw_list = args.get("image_paths")
            if isinstance(raw_list, list):
                candidates.extend(str(p) for p in raw_list)

        media: list[dict] = []
        for cand in candidates:
            path = Path(cand)
            if not path.is_absolute():
                path = project_root() / path
            saved = images_mod.save_validated_copy(path, media_dir)
            if saved is not None:
                media.append({"src": str(path), "url": f"{url_prefix}/{saved.name}"})
        if media:
            block["media"] = media
        out.append(block)
    return out


def make_turn(current_turn: dict, hop: int, model: str = "") -> dict:
    """The one persisted-turn dict every stage records per completed agent turn.

    ``model`` records which model produced this turn so the trajectory can show
    prewalk swaps and user-initiated model switches (empty for legacy turns)."""
    turn = {
        "hop": hop,
        "text": current_turn.get("text", ""),
        "thinking": current_turn.get("thinking", ""),
        "tool_blocks": list(current_turn.get("tool_blocks", [])),
        "elapsed": current_turn.get("elapsed"),
        "at": time.time(),
    }
    if model:
        turn["model"] = model
    return turn


class TurnPersister:
    """Incrementally append trajectory entries into a JsonState.

    Replaces the hand-rolled ``existing + new_turns`` write-back closures: the
    pre-existing entries are snapshotted once (from a fresh read unless
    ``existing`` is given), new entries accumulate in memory, and every append
    rewrites ``existing + new`` under the state flock. ``subpath``
    (e.g. ``["exchanges", 2]``) targets a nested list instead of the top-level
    ``key`` — the chat ``exchanges[idx]["turns"]`` case. ``post`` (optional)
    runs after each write for derived counters (s04/s05's ``total_turns``).
    """

    def __init__(
        self,
        state: JsonState,
        key: str = "turns",
        subpath: list | None = None,
        existing: list[dict] | None = None,
        post: Callable[[dict], None] | None = None,
        media_dir: Path | None = None,
        media_url_prefix: str = "",
    ):
        self.state = state
        self.key = key
        self.subpath = list(subpath) if subpath else []
        self.existing = list(existing) if existing is not None else self._snapshot()
        self.new: list[dict] = []
        self.post = post
        # The model each about-to-run hop uses; stamped onto every persisted
        # turn so the trajectory can render model switches (set before each hop).
        self.current_model: str = ""
        self.media_dir = media_dir
        self.media_url_prefix = media_url_prefix

    def _snapshot(self) -> list[dict]:
        node = self.state.read()
        for part in self.subpath:
            node = node[part]
        return list(node.get(self.key, []))

    def append(self, entry: dict) -> None:
        self.new.append(entry)

        def _write(state: dict) -> dict:
            node = state
            for part in self.subpath:
                node = node[part]
            node[self.key] = self.existing + self.new
            if self.post is not None:
                self.post(state)
            return state

        self.state.update(_write)

    def persist(self, current_turn: dict, hop: int) -> None:
        """``persist_turn`` callback for pi_runner.run_agent_hop."""
        turn = make_turn(current_turn, hop, self.current_model)
        if self.media_dir is not None:
            turn["tool_blocks"] = attach_media_to_blocks(
                turn["tool_blocks"], self.media_dir, self.media_url_prefix
            )
        self.append(turn)

    def stamp_reason(self, reason: str) -> None:
        """Record *why* the hop that just ran ended (``swap`` / ``time_cap`` /
        ``agent_end`` / ``sentinel`` / ``stopped`` / ``empty``) onto the last
        turn it persisted, so the trajectory can explain why the next turn
        exists. No-op when nothing was persisted this hop (e.g. an empty hop)."""
        if not reason or not self.new:
            return
        self.new[-1]["reason"] = reason

        def _write(state: dict) -> dict:
            node = state
            for part in self.subpath:
                node = node[part]
            node[self.key] = self.existing + self.new
            if self.post is not None:
                self.post(state)
            return state

        self.state.update(_write)

    def text(self) -> str:
        """Combined assistant text of the entries appended so far."""
        return "\n\n".join(t["text"] for t in self.new if t.get("text")).strip()


def turn_persister(
    state: JsonState,
    key: str = "turns",
    subpath: list | None = None,
    existing: list[dict] | None = None,
    post: Callable[[dict], None] | None = None,
    media_dir: Path | None = None,
    media_url_prefix: str = "",
) -> TurnPersister:
    """A TurnPersister bound to ``state`` (see the class docstring)."""
    return TurnPersister(
        state, key=key, subpath=subpath, existing=existing, post=post,
        media_dir=media_dir, media_url_prefix=media_url_prefix,
    )


def prompt_recorder(
    persister: TurnPersister,
    label: str,
    template: str,
    original: str | None = None,
    media: "list[dict] | Callable[[], list[dict]] | None" = None,
) -> Callable[[dict], None]:
    """``record_prompt`` callback: tag the compiled-prompt entry and append it.

    ``media`` (optional, a list or a callable returning one — a callable is
    read at record time so late-added attachments are picked up) attaches
    prompt-attachment thumbnails to the entry for the UI; the compiled prompt
    text itself is unchanged."""

    def record(entry: dict) -> None:
        entry.setdefault("label", label)
        entry["template"] = template
        if original is not None:
            entry["original"] = original
        rows = media() if callable(media) else media
        if rows:
            entry["media"] = rows
        persister.append(entry)

    return record


def error_recorder(
    state: JsonState, key: str = "errors", subpath: list | None = None
) -> Callable[[dict], int]:
    """``record_error`` callback for the pi runners: append a durable,
    timestamped error row (``{"kind": "error", "at": ..., **entry}`` — the
    entry supplies message/detail/rc/attempt/phase) to ``state[...][key]`` and
    return the new total count (the runners use it for the error-budget cap).
    ``subpath`` (e.g. ``["exchanges", 2]``) targets a nested dict, mirroring
    :class:`TurnPersister` for the chat ``exchanges[idx]["errors"]`` case."""

    def record(entry: dict) -> int:
        row = {"kind": "error", "at": time.time(), **entry}
        total = 0

        def _write(s: dict) -> dict:
            nonlocal total
            node = s
            for part in subpath or []:
                node = node[part]
            rows = node.setdefault(key, [])
            rows.append(row)
            total = len(rows)
            return s

        state.update(_write)
        return total

    return record


def grant_error_budget(state: dict) -> None:
    """Manual-continue budget reset (retry_from_error): another
    MAX_TOTAL_ERRORS errors on top of the rows recorded so far, and the
    over-budget banner flag cleared. The durable ``errors`` rows are kept."""
    state["error_budget"] = len(state.get("errors", [])) + MAX_TOTAL_ERRORS
    state["error_over_budget"] = False



@contextmanager
def record_chat_errors(state: JsonState, *, log_message: str = "chat failed") -> Iterator[None]:
    """Canonical worker-step error write (chat variant): phase back to idle, and
    an intentional STOP is not dressed up as an error — a kill triggered by STOP
    surfaces as an exception on some paths."""
    try:
        yield
    except Exception as e:
        logger.exception("%s", log_message)

        def _fail(s: dict) -> dict:
            s["phase"] = "idle"
            if not s.get("stop_requested"):
                s["error"] = str(e)
                s["error_detail"] = getattr(e, "detail", "")
                s["error_over_budget"] = bool(getattr(e, "over_budget", False))
            s["stop_requested"] = False
            return s

        state.update(_fail)
