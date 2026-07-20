"""One-off vision calls: the shared primitive behind the ``analyze_image``
tool and the prompt-attachment auto-description.

A single global default model, persisted at
``.pi/crack/harness/vision_config.json`` (same harness dir as the models cache
and stage overrides), customizable from the /settings page. No run/session
state machine — each call is a synchronous one-off ``pi`` process via
:func:`pi_proc.arun_pi_text` with ``@<path>`` image args.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path

from crack_server import pi_proc
from crack_server.paths import harness_dir

logger = logging.getLogger("uvicorn.error")

# The only model in the current catalog with input: ["text", "image"].
DEFAULT_VISION_MODEL = "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning"

# Used to auto-describe prompt attachments at upload time.
DESCRIBE_PROMPT = (
    "Describe this image concisely (2-3 sentences), noting anything relevant "
    "to a software task: screenshots, diagrams, error messages, UI elements."
)


def _config_path() -> Path:
    return harness_dir() / "vision_config.json"


def _config_dict() -> dict:
    path = _config_path()
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}
    return data if isinstance(data, dict) else {}


def vision_model() -> str:
    """The configured vision model, or the global default."""
    return _config_dict().get("model") or DEFAULT_VISION_MODEL


def set_vision_model(model_id: str) -> None:
    path = _config_path()
    data = _config_dict()
    data["model"] = model_id
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


async def analyze(prompt: str, image_paths: list[Path]) -> str:
    """Ask the vision model ``prompt`` about ``image_paths``; return its text.

    Paths must be validated by the caller (existence + real image) — this is a
    thin wrapper over the one-off pi text runner.
    """
    text, _elapsed = await pi_proc.arun_pi_text(
        prompt,
        log_prefix="vision",
        model=vision_model(),
        image_paths=image_paths,
    )
    return text
