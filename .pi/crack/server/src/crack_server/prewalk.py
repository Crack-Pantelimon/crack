"""Prewalk core: the model-swap state machine shared by the unscripted chat
engine and the sub-agent run loop.

Prewalk (https://stencil.so/blog/prewalk) starts a task on a frontier *planner*
model with a hidden planning instruction, has it capture the plan as a todo
list, and — the moment it lands its first edit — swaps to a cheaper
*implementer* model while pruning the planning instruction from context. The
cheap model inherits a live trajectory (exploration done, todo list started,
one edit landed), not a plan document.

This module owns only the *decisions* (which model this hop runs on, whether to
inject the hidden instruction and watch for the swap, and how to read the todo
list for nudges). Each caller keeps its own outer loop, state file, session
dir, and terminal/finish semantics — a chat ends an exchange when the model
stops; a sub-agent ends when its report is written. The swap itself happens in
``pi_proc`` (reason ``"swap"``); callers react by calling :func:`mark_swapped`.

State schema (a plain dict — run state, or a chat's synthesized config dict):
``plan: bool``, ``planner_model``/``implementer_model`` (plan mode),
``model`` (non-plan / single model), ``prewalk_phase: "planning" |
"implementing"`` (runtime; survives reloads), ``swapped_at: float``.
"""

from __future__ import annotations

import re
from pathlib import Path

from crack_server import paths

# Sensible fallback when a model field is unset (matches the historic
# sub-agent DEFAULT_MODEL / a widely-available nvidia model).
DEFAULT_MODEL = "nvidia/nemotron-3-ultra-550b-a55b"

PLANNING = "planning"
IMPLEMENTING = "implementing"

# Where the hidden planning instruction lives (the single coder persona's
# editable template). Loaded lazily so a missing file falls back gracefully.
_PLAN_INSTRUCTION_FALLBACK = (
    "You are in PLANNING mode (the user cannot see this instruction). "
    "Plan deeply first: explore the code and understand the task. Then capture "
    "the plan as a todo list via the `todo` tool (action=write) — up to ~12 "
    "concrete, independently-verifiable steps. Only once the todo list is "
    "written, begin executing. Make your edits with the `edit` tool (existing "
    "files) and `write` tool (new files) — never edit files through bash. Stop "
    "planning and start the moment you are confident enough to make your first "
    "edit."
)


def plan_instruction(persona_slug: str = "coder") -> str:
    """The hidden planner append, read from the persona's
    ``plan_instruction.md`` (customizable), or a built-in fallback."""
    try:
        path = paths.sub_agent_persona_dir(persona_slug) / "plan_instruction.md"
        if path.is_file():
            text = path.read_text(encoding="utf-8").strip()
            if text:
                return text
    except (OSError, ValueError):
        pass
    return _PLAN_INSTRUCTION_FALLBACK


# -- phase / model selection --------------------------------------------------
#
# The prewalk phase is *derived* from the persisted turns, never stored: a plan
# run has swapped (→ implementing) once a todo list exists and an edit/write has
# landed after it — the exact condition pi_proc's swap watch fires on. Deriving
# it keeps reloads and fresh exchanges correct for free: mid-exchange turns
# already show the swap, a brand-new exchange starts with none.


def swapped_already(turns: list[dict]) -> bool:
    """True once the trajectory shows a todo followed by an edit/write — i.e.
    the prewalk swap point has been passed."""
    seen_todo = False
    for turn in turns or []:
        names = [str(b.get("name", "")) for b in turn.get("tool_blocks") or []]
        if "todo" in names:
            seen_todo = True
        if seen_todo and any(n in ("edit", "write") for n in names):
            return True
    return False


def current_phase(st: dict, turns: list[dict]) -> str:
    """The prewalk phase for the *next* hop. Non-plan runs are always
    ``implementing``; plan runs are ``planning`` until the swap shows up in the
    turns, then ``implementing``."""
    if not st.get("plan"):
        return IMPLEMENTING
    return IMPLEMENTING if swapped_already(turns) else PLANNING


def model_for_phase(st: dict, turns: list[dict]) -> str:
    """Which model this hop runs on, from the run's locked model choices."""
    if not st.get("plan"):
        return st.get("model") or DEFAULT_MODEL
    if current_phase(st, turns) == PLANNING:
        return st.get("planner_model") or st.get("model") or DEFAULT_MODEL
    return st.get("implementer_model") or st.get("model") or DEFAULT_MODEL


def hop_prewalk_kwargs(st: dict, turns: list[dict], persona_slug: str = "coder") -> dict:
    """The ``arun_agent_hop`` prewalk kwargs for this hop: while planning,
    inject the hidden instruction and watch for the first-edit swap (seeding
    ``todo_already`` from prior hops); while implementing, nothing (the append
    is gone → the instruction is pruned from the cheap model's context)."""
    if current_phase(st, turns) != PLANNING:
        return {}
    return {
        "append_system_prompt": plan_instruction(persona_slug),
        "swap_after_edit": True,
        "todo_already": todo_exists(turns),
    }


# -- todo-list reading (for the swap gate's mirror + nudges) ------------------

# Matches the todo tool's plain-text rendering: ``[x] #3 add a test``.
_TODO_LINE = re.compile(r"^\[( |x)\]\s+#(\d+)\s+(.*)$")


def _latest_todo_output(turns: list[dict]) -> str | None:
    """The text output of the most recent ``todo`` tool call across turns."""
    for turn in reversed(turns or []):
        for block in reversed(turn.get("tool_blocks") or []):
            if block.get("name") == "todo" and block.get("output"):
                return str(block["output"])
    return None


def todo_exists(turns: list[dict]) -> bool:
    return _latest_todo_output(turns) is not None


def open_todos(turns: list[dict]) -> list[str]:
    """Still-open todo items as ``"#2 add rate limiter"`` strings (for nudges)."""
    out = _latest_todo_output(turns)
    if not out:
        return []
    items: list[str] = []
    for line in out.splitlines():
        m = _TODO_LINE.match(line.strip())
        if m and m.group(1) != "x":
            items.append(f"#{m.group(2)} {m.group(3)}".strip())
    return items


def nudge_text(turns: list[dict]) -> str:
    """The todo-aware implementation nudge naming the open items, or a generic
    one when no parseable todo list exists."""
    items = open_todos(turns)
    if items:
        listed = ", ".join(items)
        return (
            "You still have open todo items: "
            f"{listed}. Continue — mark each done with the `todo` tool "
            "(action=toggle) as you finish it, and write your report / reply "
            "only once the list is clear. Use `edit`/`write` for file changes, "
            "never bash."
        )
    return (
        "You have not finished. Continue the task; use `edit` for existing "
        "files and `write` for new files (never edit via bash), and keep your "
        "todo list updated. Reply only when the work is genuinely complete."
    )
