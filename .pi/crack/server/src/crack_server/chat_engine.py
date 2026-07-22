"""Chat engine: the exchange runner shared by the unscripted chats
(``chats.run_chat``) and the Finished stage's review-session chat
(``S06Finished._run_chat``).

Unscripted chats run as **prewalk coder** agents: with ``plan=True`` the first
hop runs on the planner model with a hidden planning instruction and a
swap-watch; the moment the model lands its first edit the hop ends with reason
``"swap"`` and the exchange resumes on the implementer model with the
instruction pruned (see :mod:`crack_server.prewalk`). Non-plan chats run every
hop on a single model. Either way the exchange keeps hopping (bounded) until the
model finishes with its todo list clear.

The Finished-stage review chat is *not* a coder: it passes the defaults
(``plan=False``, ``max_hops=1``), giving the original single-hop behavior.

State shape: ``state["exchanges"]`` is a list of ``{"user": str, "turns": []}``;
the agent's turns for the latest exchange are persisted into
``exchanges[-1]["turns"]`` via the shared TurnPersister.
"""

from __future__ import annotations

import logging
import time
from collections.abc import Awaitable
from pathlib import Path
from typing import Callable

from crack_server import pi_runner, prewalk
from crack_server.ratelimit import RESUME_MESSAGE
from crack_server.state import JsonState
from crack_server.steprun import (
    error_recorder,
    prompt_recorder,
    record_chat_errors,
    turn_persister,
)

logger = logging.getLogger("uvicorn.error")

# Bounds for a prewalk chat exchange: the planner hop + the implementer hop +
# a few completion nudges. A non-plan exchange normally settles in one hop.
MAX_CHAT_HOPS = 8
MAX_CHAT_NUDGES = 2


def run_exchange_sync(**kwargs) -> None:
    """Sync wrapper over :func:`run_exchange` for thread-based callers (the
    Finished stage's review chat, dispatched via ``asyncio.to_thread``).
    Must NOT be called from inside a running event loop."""
    import asyncio

    return asyncio.run(run_exchange(**kwargs))


async def run_exchange(
    *,
    state: JsonState,
    ident: str,
    message_builder: Callable[[str], str],
    record_template: str,
    log_prefix: str,
    model: str,
    session_id: str,
    sessions_dir: Path,
    tools: str | None,
    timeout_seconds: int,
    hop_kwargs: dict | None = None,
    pre_stop_check: Callable[[], bool] | None = None,
    on_first_exchange: "Callable[[str], Awaitable[None]] | None" = None,
    on_no_exchanges: Callable[[], None] | None = None,
    stopped_phase: str = "idle",
    env_extra: dict[str, str] | None = None,
    media_dir: Path | None = None,
    media_url_prefix: str = "",
    plan: bool = False,
    planner_model: str = "",
    implementer_model: str = "",
    max_hops: int = 1,
    persona_slug: str = "coder",
) -> None:
    """Run the agent for the latest entry in ``state["exchanges"]``.

    ``message_builder`` compiles the exchange's raw user text into the first
    hop's message (identity for unscripted chats, the ``chat.md`` template for
    the Finished stage). With ``plan``/``planner_model``/``implementer_model``
    set and ``max_hops`` > 1 the exchange runs the prewalk loop; the defaults
    (no plan, ``max_hops=1``) reproduce the original single-hop behavior for the
    Finished-stage caller. ``hop_kwargs`` carries ``pid_file``/``stop_check``/
    ``waiting_check`` through to the hop runner. The prewalk phase persists in
    ``state["prewalk_phase"]`` so a mid-exchange reload resumes on the right
    model.
    """
    start = time.monotonic()
    with record_chat_errors(state, log_message=f"{log_prefix}: exchange failed for {ident}"):
        exchanges = state.read().get("exchanges", [])
        if not exchanges:
            if on_no_exchanges is not None:
                on_no_exchanges()
            return
        idx = len(exchanges) - 1
        user_msg = exchanges[idx].get("user", "")
        first_message = message_builder(user_msg)

        if idx == 0 and on_first_exchange is not None:
            await on_first_exchange(user_msg)

        persister = turn_persister(
            state, subpath=["exchanges", idx],
            media_dir=media_dir, media_url_prefix=media_url_prefix,
        )

        if pre_stop_check is not None and pre_stop_check():
            reason = "stopped"
        else:
            reason = await _run_prewalk_loop(
                state=state, idx=idx, persister=persister, log_prefix=log_prefix,
                model=model, planner_model=planner_model,
                implementer_model=implementer_model, plan=plan,
                persona_slug=persona_slug, session_id=session_id,
                sessions_dir=sessions_dir, tools=tools, timeout_seconds=timeout_seconds,
                start=start, first_message=first_message, user_msg=user_msg,
                record_template=record_template, hop_kwargs=hop_kwargs,
                env_extra=env_extra, max_hops=max_hops,
            )

        def _finish(s: dict) -> dict:
            s["phase"] = stopped_phase if reason == "stopped" else "idle"
            s["stop_requested"] = False
            if reason == "empty":
                s["error"] = "model returned empty responses"
                s["error_detail"] = ""
            return s

        state.update(_finish)
        logger.info("%s: exchange %d done for %s (reason=%s)", log_prefix, idx, ident, reason)


async def _run_prewalk_loop(
    *,
    state: JsonState,
    idx: int,
    persister,
    log_prefix: str,
    model: str,
    planner_model: str,
    implementer_model: str,
    plan: bool,
    persona_slug: str,
    session_id: str,
    sessions_dir: Path,
    tools: str | None,
    timeout_seconds: int,
    start: float,
    first_message: str,
    user_msg: str,
    record_template: str,
    hop_kwargs: dict | None,
    env_extra: dict[str, str] | None,
    max_hops: int,
) -> str:
    """Drive one exchange to completion across prewalk hops; return the final
    stop reason."""
    message = first_message
    nudge_count = 0
    reason = "agent_end"

    def _turns() -> list[dict]:
        return state.read()["exchanges"][idx].get("turns", [])

    for hop in range(1, max_hops + 1):
        st = {
            "plan": plan,
            "planner_model": planner_model,
            "implementer_model": implementer_model,
            "model": model,
        }
        turns_now = _turns()
        hop_model = prewalk.model_for_phase(st, turns_now)
        pw_kwargs = prewalk.hop_prewalk_kwargs(st, turns_now, persona_slug)
        # Stamp this hop's model onto every turn it persists (trajectory swaps).
        persister.current_model = hop_model

        if hop == 1:
            record = prompt_recorder(persister, "chat", record_template, original=user_msg)
        elif message == RESUME_MESSAGE:
            record = prompt_recorder(persister, "resume", "")
        else:
            record = prompt_recorder(persister, "nudge", "")

        reason = await pi_runner.arun_agent_hop(
            log_prefix=log_prefix,
            model=hop_model,
            session_id=session_id,
            sessions_dir=sessions_dir,
            tools=tools,
            message=message,
            start=start,
            sentinel=None,
            timeout_seconds=timeout_seconds,
            persist_turn=persister.persist,
            hop=hop,
            record_prompt=record,
            record_error=error_recorder(state, subpath=["exchanges", idx]),
            env_extra=env_extra,
            **(hop_kwargs or {}),
            **pw_kwargs,
        )
        # Record why this hop ended on its last persisted turn (trajectory note).
        persister.stamp_reason(reason)

        if reason in ("stopped", "empty"):
            break
        if reason == "swap":
            # Phase auto-derives to implementing from the now-persisted edit
            # turn; just resume the same session on the implementer model.
            message = RESUME_MESSAGE
            continue

        # Natural end of the model's turn (agent_end/time_cap/sentinel). Resume
        # when either the todo list still has open items *or* the last persisted
        # turn of this exchange made tool calls (continue-if-tools). Bounded by
        # MAX_CHAT_NUDGES / max_hops so a chatty tool-caller can't loop forever.
        opens = prewalk.open_todos(_turns())
        last_turn = next(
            (t for t in reversed(_turns()) if not t.get("kind")),
            None,
        )
        last_had_tools = bool(last_turn and (last_turn.get("tool_blocks") or []))
        if (
            (opens or last_had_tools)
            and nudge_count < MAX_CHAT_NUDGES
            and hop < max_hops
        ):
            message = (
                prewalk.nudge_text(_turns()) if opens else RESUME_MESSAGE
            )
            nudge_count += 1
            continue
        break

    return reason
