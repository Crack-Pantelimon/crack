"""ask_user: hop-terminating human questions (generalizes the planner's
``awaiting_answers`` pattern to every sub-agent run and to chat parents).

For a **run** parent the hop ends cleanly — no pi process is held while the
human thinks, no nudges accrue, and the orphan sweep skips the
``awaiting_user`` phase. When the answer arrives, :func:`answer` enqueues a
fresh resume hop carrying the answer as the user message. For a **chat**
parent the question is recorded for prominent UI rendering; the chat's normal
input is the answer channel (chats already idle between exchanges).
"""

from __future__ import annotations

import logging
import time

from crack_server import paths
from crack_server.sub_agents import registry
from crack_server.sub_agents.base import TERMINAL_PHASES

logger = logging.getLogger("uvicorn.error")

AWAITING_USER_PHASE = "awaiting_user"


def ask(
    *,
    chat_id: str,
    parent_kind: str,
    parent_id: str,
    question: str,
    choices: list[str] | None = None,
) -> str:
    """Record a pending question for the parent. Returns "awaiting_user" for a
    run parent (the run suspends) or "recorded" for a chat parent."""
    payload = {
        "question": question,
        "choices": list(choices or []),
        "asked_at": time.time(),
    }
    if parent_kind == "run":
        state_obj = paths.run_state_by_id(parent_id)
        state = state_obj.read()
        if not state:
            raise ValueError(f"run not found: {parent_id}")
        if state.get("phase") in TERMINAL_PHASES:
            raise ValueError(f"run is terminal ({state.get('phase')}): {parent_id}")

        def _ask(s: dict) -> dict:
            s["phase"] = AWAITING_USER_PHASE
            s["pending_question"] = payload
            return s

        state_obj.update(_ask)
        logger.info("ask_user: run %s suspends for a human answer", parent_id)
        return AWAITING_USER_PHASE

    def _ask_chat(s: dict) -> dict:
        s["pending_question"] = payload
        return s

    paths.chat_state(chat_id).update(_ask_chat)
    logger.info("ask_user: chat %s records a question for the human", chat_id)
    return "recorded"


def answer(chat_id: str, run_id: str, answer_text: str) -> bool:
    """Store the human's answer and enqueue a resume hop that receives it as
    the user message. False when the run is not awaiting an answer."""
    state_obj = paths.run_state_by_id(run_id)
    state = state_obj.read()
    if not state or state.get("phase") != AWAITING_USER_PHASE:
        return False
    question = (state.get("pending_question") or {}).get("question", "")
    message = (
        "The user answered your question.\n\n"
        f"Question: {question}\n\nAnswer: {answer_text}"
    )

    def _record(s: dict) -> dict:
        qa = list(s.get("user_qa") or [])
        qa.append({"question": question, "answer": answer_text, "at": time.time()})
        s["user_qa"] = qa
        s.pop("pending_question", None)
        s["phase"] = "resuming"
        return s

    updated = state_obj.update(_record)
    persona = registry.get(state.get("persona", ""))
    if persona is not None:
        persona.enqueue_step(
            run_id,
            "run",
            {
                "run_id": run_id,
                "started_token": updated.get("started_token"),
                "user_answer": message,
            },
        )
    logger.info("ask_user: run %s resumed with the human's answer", run_id)
    return True
