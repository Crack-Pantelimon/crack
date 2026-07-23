"""Global agent model defaults, persisted at
``.pi/crack/harness/agent_config.json`` (same harness dir as the models cache
and vision config), customizable from the /settings page.

Three picks seed every new chat's locked model dropdowns and every spawned
sub-agent:

- ``plan_planner``     — the frontier planner in prewalk plan mode;
- ``plan_implementer`` — the cheap implementer after the swap;
- ``nonplan``          — the single model used when plan mode is off.
"""

from __future__ import annotations

import json
from pathlib import Path

from crack_server.paths import harness_dir

DEFAULT_PLAN_PLANNER = "nvidia/z-ai/glm-5.2"
DEFAULT_PLAN_IMPLEMENTER = "nvidia/nemotron-3-ultra-550b-a55b"
DEFAULT_NONPLAN = "nvidia/z-ai/glm-5.2"

_KEYS = {
    "plan_planner": DEFAULT_PLAN_PLANNER,
    "plan_implementer": DEFAULT_PLAN_IMPLEMENTER,
    "nonplan": DEFAULT_NONPLAN,
}


def _config_path() -> Path:
    return harness_dir() / "agent_config.json"


def _config_dict() -> dict:
    path = _config_path()
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}
    return data if isinstance(data, dict) else {}


def get_model(kind: str) -> str:
    """The configured model for ``kind`` (plan_planner/plan_implementer/nonplan),
    or its built-in default."""
    if kind not in _KEYS:
        raise ValueError(f"unknown model kind: {kind}")
    return _config_dict().get(kind) or _KEYS[kind]


def set_model(kind: str, model_id: str) -> None:
    if kind not in _KEYS:
        raise ValueError(f"unknown model kind: {kind}")
    path = _config_path()
    data = _config_dict()
    data[kind] = model_id
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def plan_planner_model() -> str:
    return get_model("plan_planner")


def plan_implementer_model() -> str:
    return get_model("plan_implementer")


def nonplan_model() -> str:
    return get_model("nonplan")


# ---------------------------------------------------------------------------
# RAG first-hop injection (Plan 30 Part 5)
# ---------------------------------------------------------------------------

_RAG_DEFAULTS = {
    "first_hop_enabled": True,
    "first_hop_top_k": 5,
    # Live scores on nomic-embed-text over our index cluster ~0.02–0.05.
    "first_hop_min_score": 0.02,
    "first_hop_max_chars": 12000,
}


def _rag_config_path() -> Path:
    return harness_dir() / "rag.json"


def rag_config() -> dict:
    """Merged RAG injection settings (harness ``rag.json`` over defaults)."""
    path = _rag_config_path()
    data: dict = {}
    if path.is_file():
        try:
            raw = json.loads(path.read_text(encoding="utf-8"))
            if isinstance(raw, dict):
                data = raw
        except (json.JSONDecodeError, OSError):
            pass
    # Accept legacy first_turn_* keys from early plan drafts.
    legacy = {
        "first_hop_enabled": data.pop("first_turn_enabled", None),
        "first_hop_top_k": data.pop("first_turn_top_k", None),
        "first_hop_min_score": data.pop("first_turn_min_score", None),
        "first_hop_max_chars": data.pop("first_turn_max_chars", None),
    }
    out = dict(_RAG_DEFAULTS)
    for key, default in _RAG_DEFAULTS.items():
        val = data.get(key, legacy.get(key))
        if val is not None:
            out[key] = val
    return out
