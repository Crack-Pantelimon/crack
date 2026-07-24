"""Vision API: the server side of the ``analyze_image`` pi tool."""

from __future__ import annotations

import logging
from pathlib import Path

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import JSONResponse

from crack_server import images, sandbox, vision
from crack_server.paths import project_root

router = APIRouter()

logger = logging.getLogger("uvicorn.error")


async def _resolve_sandbox_name(body: dict) -> str | None:
    """Return the sandbox container name when the caller is running in one.

    The extension sends ``conv_id`` (chat id or run id — whichever owns the
    sandbox the tool is executing in). Host-only / sandbox-disabled calls omit
    it and fall through to local filesystem validation.
    """
    if not sandbox.sandbox_enabled():
        return None
    conv_id = str(body.get("conv_id") or "").strip()
    if not conv_id:
        return None
    name = sandbox.sandbox_name(conv_id)
    if not await sandbox.container_exists(name):
        raise HTTPException(
            status_code=400,
            detail=f"sandbox for conv_id={conv_id!r} is not running",
        )
    return name


@router.post("/api/vision/analyze")
async def api_vision_analyze(request: Request) -> JSONResponse:
    """Body: ``{prompt: str, image_paths: [str], conv_id?: str}`` → ``{"text": ...}``.

    Every path is re-validated server-side (existence + imagemagick identify);
    any bad path fails the whole call with a 400 listing exactly which paths
    were not found vs. not valid images.

    When ``conv_id`` is set (agent running inside a chat/run sandbox), paths are
    validated and the vision ``pi`` call is executed *inside that sandbox* so
    files created during the hop (e.g. screenshots under ``/workspace``) are
    visible — the host filesystem never sees the overlay upper.
    """
    try:
        body = await request.json()
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"invalid JSON body: {e}") from e
    if not isinstance(body, dict):
        raise HTTPException(status_code=400, detail="JSON object required")

    prompt = str(body.get("prompt") or "").strip()
    raw_paths = body.get("image_paths")
    if not prompt:
        raise HTTPException(status_code=400, detail="prompt is required")
    if not (
        isinstance(raw_paths, list) and raw_paths
        and all(isinstance(p, str) for p in raw_paths)
    ):
        raise HTTPException(status_code=400, detail="image_paths must be a non-empty list of strings")

    sbx = await _resolve_sandbox_name(body)
    paths: list[Path] = []
    missing: list[str] = []
    invalid: list[str] = []

    if sbx:
        for raw in raw_paths:
            p = Path(raw)
            if not p.is_absolute():
                p = Path("/workspace") / p
            if not await sandbox.path_is_file(sbx, str(p)):
                missing.append(raw)
            elif not await sandbox.identify_ok(sbx, str(p)):
                invalid.append(raw)
            else:
                paths.append(p)
    else:
        root = project_root()
        for raw in raw_paths:
            p = Path(raw)
            if not p.is_absolute():
                p = root / p
            if not p.is_file():
                missing.append(raw)
            elif not images.identify_ok(p):
                invalid.append(raw)
            else:
                paths.append(p)

    if missing or invalid:
        parts = []
        if missing:
            parts.append("not found: " + ", ".join(missing))
        if invalid:
            parts.append("not a valid image: " + ", ".join(invalid))
        raise HTTPException(status_code=400, detail="invalid image paths (" + "; ".join(parts) + ")")

    try:
        text = await vision.analyze(prompt, paths, sandbox=sbx)
    except Exception as e:
        logger.exception("vision: analyze failed")
        raise HTTPException(status_code=500, detail=str(e)) from e
    return JSONResponse({"text": text})
