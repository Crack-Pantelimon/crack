"""Shared stage-step machinery (plan 4.3 A2).

The turn-trajectory persistence, the hop-loop drivers, and the canonical
error-state write that s02–s06 and the chat engine used to copy-paste:

- :func:`make_turn` / :class:`TurnPersister` — the one turn-dict constructor
  plus the ``existing + new`` write-back closure (``subpath`` covers the chat
  ``exchanges[idx]["turns"]`` case).
- :func:`hop_loop` — the continue-nudge loop driver shared by the long-running
  agent stages (s04/s05); :func:`hop_with_nudge` — the single flow-control
  nudge shared by the interviewing stages (s02/s03).
- :func:`record_errors` / :func:`record_chat_errors` — the canonical
  ``except Exception → error state`` write for worker steps (stage and chat
  variants).

Everything builds on :class:`state.JsonState` — persistence goes through
``update(fn)`` exactly like the hand-rolled closures these helpers replace.
"""

from __future__ import annotations

import logging
import time
from contextlib import contextmanager
from typing import Callable, Iterator

from crack_server.state import JsonState

logger = logging.getLogger("uvicorn.error")

# Raised (as RuntimeError) by every stage when pi came back contentless.
EMPTY_TURNS_MESSAGE = "pi returned empty responses (no content in any turn)"


def make_turn(current_turn: dict, hop: int) -> dict:
    """The one persisted-turn dict every stage records per completed agent turn."""
    return {
        "hop": hop,
        "text": current_turn.get("text", ""),
        "thinking": current_turn.get("thinking", ""),
        "tool_blocks": list(current_turn.get("tool_blocks", [])),
        "elapsed": current_turn.get("elapsed"),
    }


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
    ):
        self.state = state
        self.key = key
        self.subpath = list(subpath) if subpath else []
        self.existing = list(existing) if existing is not None else self._snapshot()
        self.new: list[dict] = []
        self.post = post

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
        self.append(make_turn(current_turn, hop))

    def text(self) -> str:
        """Combined assistant text of the entries appended so far."""
        return "\n\n".join(t["text"] for t in self.new if t.get("text")).strip()


def turn_persister(
    state: JsonState,
    key: str = "turns",
    subpath: list | None = None,
    existing: list[dict] | None = None,
    post: Callable[[dict], None] | None = None,
) -> TurnPersister:
    """A TurnPersister bound to ``state`` (see the class docstring)."""
    return TurnPersister(state, key=key, subpath=subpath, existing=existing, post=post)


def prompt_recorder(
    persister: TurnPersister,
    label: str,
    template: str,
    original: str | None = None,
) -> Callable[[dict], None]:
    """``record_prompt`` callback: tag the compiled-prompt entry and append it."""

    def record(entry: dict) -> None:
        entry.setdefault("label", label)
        entry["template"] = template
        if original is not None:
            entry["original"] = original
        persister.append(entry)

    return record


def bump_total_turns(state: dict) -> None:
    """``post`` hook keeping ``total_turns`` in sync with the turns list (s04/s05)."""
    state["total_turns"] = len(state["turns"])


def mark_run_stopped(state: JsonState) -> None:
    """``on_stopped`` callback for hop_loop (s04/s05): phase + stop_reason stopped."""

    def _stopped(s: dict) -> dict:
        s["phase"] = "stopped"
        s["stop_reason"] = "stopped"
        return s

    state.update(_stopped)


def hop_loop(
    *,
    start: float,
    timeout_seconds: int,
    message: str,
    run_hop: Callable[[str, int], str],
    continue_message: Callable[[], str],
    before_round: Callable[[int], str | None] | None = None,
    after_hop: Callable[[str, int], None] | None = None,
    on_stopped: Callable[[], None],
) -> str | None:
    """Continue-nudge loop driver shared by the long-running agent stages
    (s04/s05): run hops until the sentinel, the wall clock, or an external stop,
    nudging with ``continue_message()`` after every ``agent_end``.

    ``run_hop(message, round_n)`` runs one hop (round_n is 1-based) and returns
    its stop reason. ``before_round(round_n)`` may substitute that round's
    message (s04's todo reminder). ``after_hop(reason, round_n)`` runs
    stage-specific per-hop checks (s04's model fallback). Returns the stop
    reason ("sentinel"/"time_cap"), or None after ``on_stopped()`` ran for an
    external stop. Raises RuntimeError on an "empty" hop.
    """
    round_n = 0
    while True:
        if time.monotonic() - start > timeout_seconds:
            return "time_cap"
        round_n += 1
        if before_round is not None:
            override = before_round(round_n)
            if override is not None:
                message = override
        reason = run_hop(message, round_n)
        if after_hop is not None:
            after_hop(reason, round_n)
        if reason == "empty":
            raise RuntimeError(EMPTY_TURNS_MESSAGE)
        if reason == "stopped":
            on_stopped()
            return None
        if reason in ("sentinel", "time_cap"):
            return reason
        # agent_end → keep going with a continuation nudge.
        message = continue_message()


def hop_with_nudge(
    *,
    run_hop: Callable[[str, str, int], str],
    message: str,
    template: str,
    nudge: str,
    text_so_far: Callable[[], str],
    sentinels: tuple[str, ...] = (),
    on_stopped: Callable[[], None] | None = None,
) -> tuple[str, str]:
    """One hop plus a single flow-control nudge (not a cap) when the agent ended
    without emitting either a questions block or one of ``sentinels`` — the
    pattern shared by the interviewing stages (s02 draft, s03 critic).

    ``run_hop(message, template, hop)`` returns the hop's stop reason;
    ``text_so_far()`` is the combined text of the turns persisted so far.
    Returns ``(text, reason)``; raises RuntimeError on an "empty" hop.
    ``on_stopped`` (optional) fires when a hop came back "stopped".
    """
    from crack_server.stages.qa import parse_questions  # lazy: keep steprun leaf-ish

    reason = run_hop(message, template, 1)
    if reason == "empty":
        raise RuntimeError(EMPTY_TURNS_MESSAGE)
    if reason == "stopped" and on_stopped is not None:
        on_stopped()
    text = text_so_far()
    if (
        reason == "agent_end"
        and not parse_questions(text)
        and not any(sentinel in text for sentinel in sentinels)
    ):
        reason = run_hop(nudge, "", 2)
        if reason == "empty":
            raise RuntimeError(EMPTY_TURNS_MESSAGE)
        if reason == "stopped" and on_stopped is not None:
            on_stopped()
        text = text_so_far()
    return text, reason


@contextmanager
def record_errors(
    state: JsonState,
    step: str,
    *,
    phase_key: str = "phase",
    log_message: str = "stage step failed",
) -> Iterator[None]:
    """Canonical worker-step error write (stage variant): on exception, log it
    and land the state in error with error/error_detail/error_step/finished_at.
    The exception is swallowed, exactly like the pasted except blocks this
    replaces — a failed step must never wedge the worker's queue loop."""
    try:
        yield
    except Exception as e:
        logger.exception("%s", log_message)

        def _fail(s: dict) -> dict:
            s[phase_key] = "error"
            s["error"] = str(e)
            s["error_detail"] = getattr(e, "detail", "")
            s["error_step"] = step
            s["finished_at"] = time.time()
            return s

        state.update(_fail)


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
            s["stop_requested"] = False
            return s

        state.update(_fail)
