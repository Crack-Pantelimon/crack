"""Shared short-title generation (task titles + unscripted-chat titles).

One place owns the title-template prompt build, the nano title-model call, and
the length clamp (``TITLE_MAX_LENGTH``) so the two title surfaces can't drift
apart (plan 4.2 A7). Callers handle persistence and failure recording.
"""

from __future__ import annotations

import logging

from crack_server import pi_runner
from crack_server import ui as _ui

logger = logging.getLogger("uvicorn.error")

# Clamp applied to every generated title (the chat titles' historic limit).
TITLE_MAX_LENGTH = 80


def generate_title(content: str, *, log_prefix: str) -> str:
    """Summarize ``content`` into a short title via the nano title model.

    Returns the reply normalized (stripped, surrounding quotes removed) and
    clamped to ``TITLE_MAX_LENGTH`` chars. Raises on pi failure — the caller
    decides how to record/log it."""
    prompt = _ui._load_template("title").replace("{content}", content)
    title, _ = pi_runner.run_pi_text(
        prompt,
        log_prefix=log_prefix,
        model=pi_runner.TITLE_MODEL,
        max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
    )
    return title.strip().strip('"').strip()[:TITLE_MAX_LENGTH]
