"""Parent resume after child sub-agents complete."""

from __future__ import annotations

import logging

from crack_server.sub_agents.base import SubAgentPersona
from crack_server.sub_agents.runner import format_child_result

logger = logging.getLogger("uvicorn.error")


async def drain_children(
    run_id: str, persona: SubAgentPersona
) -> tuple[str, dict | None] | None:
    """Read+clear child_inbox, run one parent hop with the formatted results."""
    inbox: list[dict] = []

    def _take_inbox(state: dict) -> dict:
        inbox.extend(state.get("child_inbox") or [])
        state["child_inbox"] = []
        return state

    persona.state_update(run_id, _take_inbox)
    if not inbox:
        return None

    message = "\n\n---\n\n".join(format_child_result(entry) for entry in inbox)
    return await persona._run_hop(run_id, {"child_results": message})
