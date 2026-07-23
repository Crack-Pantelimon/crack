"""Parent resume after child sub-agents complete."""

from __future__ import annotations

import logging

from crack_server import paths
from crack_server.sub_agents.base import SubAgentPersona
from crack_server.sub_agents.runner import format_child_result, mark_delivered_to_parent

logger = logging.getLogger("uvicorn.error")


async def drain_children(
    run_id: str, persona: SubAgentPersona
) -> tuple[str, dict | None] | None:
    """Read+clear child_inbox, run one parent hop with the formatted results.

    A child already ``delivered_to_parent`` (its report reached this agent inline
    via wait_join) is dropped rather than re-delivered — the same double-delivery
    ledger the chat parent uses in ``chats._merge_child_inbox``. Surviving
    children are marked delivered as they are formatted into the hop message."""
    inbox: list[dict] = []

    def _take_inbox(state: dict) -> dict:
        inbox.extend(state.get("child_inbox") or [])
        state["child_inbox"] = []
        return state

    persona.state_update(run_id, _take_inbox)

    fresh: list[dict] = []
    for entry in inbox:
        cid = entry.get("run_id")
        if cid:
            try:
                if paths.run_state_by_id(cid).read().get("delivered_to_parent"):
                    continue
            except (ValueError, FileNotFoundError):
                pass
        fresh.append(entry)
        mark_delivered_to_parent(cid)

    if not fresh:
        return None

    message = "\n\n---\n\n".join(format_child_result(entry) for entry in fresh)
    return await persona._run_hop(run_id, {"child_results": message})
