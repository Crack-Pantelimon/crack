"""Prompt-image attachments: images pasted/dropped into the chat message box,
saved server-side under ``attachments/``, described at upload time by the
vision model, and woven into the next chat message by ``chats.post_message``
(one-shot: cleared after send).

The manifest is a ``JsonState`` at ``attachments/images.json`` holding
``{"images": [{id, filename, saved_path, description, uploaded_at}]}``; the id
is the content-derived saved filename (sha1[:12] + ext).
"""

from __future__ import annotations

import logging
import time
from pathlib import Path

from fastapi import HTTPException
from fastapi.responses import FileResponse

from crack_server import images, vision
from crack_server.state import JsonState
from crack_server.ui import _esc

logger = logging.getLogger("uvicorn.error")


def list_attachments(state: JsonState) -> list[dict]:
    entries = state.read().get("images")
    return [e for e in entries if isinstance(e, dict)] if isinstance(entries, list) else []


def clear(state: JsonState) -> None:
    """Empty the manifest (uploaded files stay on disk for history)."""
    state.write({"images": []})


def format_block(entries: list[dict]) -> str:
    """The attachment block prepended to a compiled prompt / chat message."""
    n = len(entries)
    lines = [f"User attached {n} image{'s' if n != 1 else ''}:"]
    for e in entries:
        lines.append(f"- {e.get('saved_path', '')}")
        lines.append(f"  - {e.get('description', '')}")
    lines.append(
        "You may use the analyze_image tool to ask further questions about these images."
    )
    lines.append("----")
    return "\n".join(lines)


async def add_attachment(
    state: JsonState, attachments_dir: Path, data: bytes, orig_name: str
) -> dict:
    """Validate + store an upload, auto-describe it via the vision model
    (best-effort), and append it to the manifest. Raises ValueError when the
    bytes are not a valid image."""
    saved = images.save_validated_upload(data, orig_name, attachments_dir)
    if saved is None:
        raise ValueError(f"not a valid image: {orig_name or 'upload'}")

    existing = list_attachments(state)
    for e in existing:
        if e.get("id") == saved.name:
            return e  # same bytes uploaded twice — keep the first entry

    description = ""
    try:
        description = await vision.analyze(vision.DESCRIBE_PROMPT, [saved])
    except Exception:
        logger.exception("attachments: auto-description failed for %s", saved)

    entry = {
        "id": saved.name,
        "filename": orig_name or saved.name,
        "saved_path": str(saved),
        "description": description,
        "uploaded_at": time.time(),
    }

    def _append(s: dict) -> dict:
        images_list = s.get("images")
        if not isinstance(images_list, list):
            images_list = []
        images_list.append(entry)
        s["images"] = images_list
        return s

    state.update(_append)
    return entry


def delete_attachment(state: JsonState, attachments_dir: Path, entry_id: str) -> bool:
    """Remove one manifest entry and its file. False when the id is unknown."""
    base = images.validate_media_filename(entry_id)  # raises ValueError
    removed = False

    def _remove(s: dict) -> dict:
        nonlocal removed
        entries = s.get("images")
        if not isinstance(entries, list):
            entries = []
        kept = [e for e in entries if e.get("id") != base]
        removed = len(kept) != len(entries)
        s["images"] = kept
        return s

    state.update(_remove)
    if removed:
        try:
            (attachments_dir / base).unlink()
        except OSError:
            pass
    return removed


def render_chip(owner: str, owner_id: str, entry: dict) -> str:
    """Thumbnail chip for the preview strip: image (click = lightbox) + delete.

    ``owner`` is ``"tasks"`` or ``"chats"`` — it selects the serve/delete URLs.
    """
    eid = _esc(str(entry.get("id", "")))
    desc = _esc(str(entry.get("description") or entry.get("filename") or ""))
    return (
        f'<span class="attachment-chip">'
        f'<img class="tool-thumb" src="/{owner}/{owner_id}/attachments/{eid}" '
        f'alt="{desc}" title="{desc}">'
        f'<button class="contrast compact-btn" '
        f'hx-delete="/api/{owner}/{owner_id}/attachments/{eid}" '
        f'hx-target="closest .attachment-chip" hx-swap="outerHTML" '
        f'title="Remove attachment">×</button>'
        f"</span>"
    )


def render_strip(owner: str, owner_id: str, state: JsonState, strip_id: str) -> str:
    """The preview-strip container with a chip per manifest entry."""
    chips = "".join(render_chip(owner, owner_id, e) for e in list_attachments(state))
    return f'<div id="{strip_id}" class="attachment-strip">{chips}</div>'


def serve_file(directory: Path, filename: str) -> FileResponse:
    """Validated FileResponse for media/attachment GET routes (404 on miss)."""
    try:
        base = images.validate_media_filename(filename)
    except ValueError as e:
        raise HTTPException(status_code=404, detail="not found") from e
    path = directory / base
    if not path.is_file():
        raise HTTPException(status_code=404, detail="not found")
    return FileResponse(path)
