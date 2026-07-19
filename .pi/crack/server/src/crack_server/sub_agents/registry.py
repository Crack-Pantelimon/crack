"""Lazy discovery of checked-in persona modules under .pi/crack/sub_agents/."""

from __future__ import annotations

import importlib.util
import logging
import sys
from pathlib import Path

from crack_server import paths
from crack_server.sub_agents.base import SubAgentPersona

logger = logging.getLogger("uvicorn.error")

_CACHE: dict[str, tuple[Path, SubAgentPersona]] = {}


def _load_persona(agent_py: Path) -> SubAgentPersona:
    resolved = agent_py.resolve()
    cache_key = str(resolved)
    if cache_key in _CACHE and _CACHE[cache_key][0] == resolved:
        return _CACHE[cache_key][1]

    module_name = f"crack_subagent_{resolved.parent.name}"
    spec = importlib.util.spec_from_file_location(module_name, resolved)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load persona module: {resolved}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    persona = getattr(module, "PERSONA", None)
    if not isinstance(persona, SubAgentPersona):
        raise RuntimeError(f"{resolved}: missing module-level PERSONA = <SubAgentPersona>()")
    _CACHE[cache_key] = (resolved, persona)
    return persona


def _discover() -> dict[str, SubAgentPersona]:
    registry: dict[str, SubAgentPersona] = {}
    base = paths.sub_agents_dir()
    if not base.is_dir():
        return registry
    for agent_py in sorted(base.glob("*/agent.py")):
        try:
            persona = _load_persona(agent_py)
        except Exception:
            logger.exception("sub_agents: failed to load %s", agent_py)
            continue
        if persona.slug in registry:
            raise RuntimeError(f"duplicate persona slug {persona.slug!r}")
        registry[persona.slug] = persona
    return registry


_REGISTRY: dict[str, SubAgentPersona] | None = None
_REGISTRY_ROOT: Path | None = None


def _registry() -> dict[str, SubAgentPersona]:
    """Return the persona registry, re-discovering when sub_agents_dir moves
    (tests monkeypatch CRACK_PI_PROJECT_ROOT per case)."""
    global _REGISTRY, _REGISTRY_ROOT
    root = paths.sub_agents_dir().resolve()
    if _REGISTRY is None or _REGISTRY_ROOT != root:
        _REGISTRY = _discover()
        _REGISTRY_ROOT = root
    return _REGISTRY


def get(slug: str) -> SubAgentPersona | None:
    return _registry().get(slug)


def list_personas() -> list[SubAgentPersona]:
    return [_registry()[slug] for slug in sorted(_registry())]


def clear_cache() -> None:
    """Test helper: drop the cached registry so the next lookup re-discovers."""
    global _REGISTRY, _REGISTRY_ROOT, _CACHE
    _REGISTRY = None
    _REGISTRY_ROOT = None
    _CACHE.clear()
