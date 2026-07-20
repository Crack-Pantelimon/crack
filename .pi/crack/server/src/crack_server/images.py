"""Image validation and per-context media copies (shared by the vision route,
turn-persistence media hook, and prompt-attachment uploads).

Validation shells out to imagemagick's ``identify`` (present in the container)
— no Pillow dependency, consistent with this codebase's shell-out convention.
"""

from __future__ import annotations

import hashlib
import logging
import re
import subprocess
from pathlib import Path

logger = logging.getLogger("uvicorn.error")

# Extensions treated as image candidates by the turn-persistence media hook.
IMAGE_EXTS = frozenset({
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".tiff", ".tif",
})

# Saved media filenames are content-derived (sha1[:12] + ext), so this is the
# strict basename check the media-serving routes apply (no path traversal).
MEDIA_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$")


def validate_media_filename(name: str) -> str:
    """Return ``name`` as a bare basename, or raise ValueError."""
    base = Path(name).name
    if not MEDIA_NAME_RE.fullmatch(base):
        raise ValueError("invalid media filename")
    return base


def identify_ok(path: Path) -> bool:
    """True when imagemagick ``identify`` accepts ``path`` as a real image."""
    try:
        result = subprocess.run(
            ["identify", str(path)],
            capture_output=True,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        logger.warning("images: identify failed to run for %s: %s", path, e)
        return False
    return result.returncode == 0


def _dest_name(digest: str, ext: str) -> str:
    return f"{digest[:12]}{ext.lower()}"


def save_validated_copy(src: Path, dest_dir: Path) -> Path | None:
    """Validate ``src`` as an image and copy it into ``dest_dir`` under a
    content-derived name (sha1 of the file bytes + original extension).

    Returns the destination path, or None on any failure (missing, unreadable,
    not a real image) — callers skip silently, per spec.
    """
    try:
        data = src.read_bytes()
    except OSError:
        return None
    if not identify_ok(src):
        return None
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / _dest_name(hashlib.sha1(data).hexdigest(), src.suffix)
    if not dest.is_file():
        try:
            dest.write_bytes(data)
        except OSError as e:
            logger.warning("images: could not save copy of %s to %s: %s", src, dest, e)
            return None
    return dest


def save_validated_upload(data: bytes, orig_name: str, dest_dir: Path) -> Path | None:
    """Validate uploaded bytes as an image and store them in ``dest_dir`` under
    a content-derived name. Returns the destination path, or None when invalid."""
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / _dest_name(hashlib.sha1(data).hexdigest(), Path(orig_name).suffix)
    try:
        dest.write_bytes(data)
    except OSError as e:
        logger.warning("images: could not save upload %s to %s: %s", orig_name, dest, e)
        return None
    if not identify_ok(dest):
        try:
            dest.unlink()
        except OSError:
            pass
        return None
    return dest
