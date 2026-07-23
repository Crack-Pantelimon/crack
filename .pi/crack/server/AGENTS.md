# crack-pi-server — the one thing to know: the venv lives in `target/`

This package (the harness/server) is managed with **Poetry**, and its
virtualenv is deliberately placed **outside the source tree**, in the
build/cache volume, via `_docker/image/Dockerfile`:

```dockerfile
ENV POETRY_VIRTUALENVS_PATH=/workspace/target/python-venvs/
```

So `poetry install` creates `/workspace/target/python-venvs/crack-pi-server-<hash>-py3.13/`,
**never** an in-project `.venv/` here. Keep it that way.

## Why this matters (sandbox forking)

Sub-agents run in per-conversation sandboxes that mount `/workspace` as a
**frozen, read-only overlay lower** (the git tree snapshot) with a writable
upper, and mount `/workspace/target` as a **separate overlay** on the shared
`crack-dev-target-dir` volume (see `sandbox.py`). That split is the whole point:

- A sub-agent that edits *this* server and runs `poetry install` gets its new/
  updated venv **copied-up into its own `target` overlay upper** — isolated to
  that conversation, discarded with it, and invisible to other sandboxes.
- If the venv lived in-project (`.venv/` under `/workspace`), it would land in
  the **frozen base tree**: it could not be written per-sandbox, would bloat the
  snapshot, and forking would break. `uv`'s in-source `.venv` is exactly why we
  use Poetry here, not uv.

Net: **source in `/workspace` (frozen, shared) · venv + build artifacts in
`/workspace/target` (per-sandbox overlay).**

## The `python` prerequisite (why it silently degraded once)

Poetry 2.x and `virtualenv` shell out to a **bare `python`**. The base image
ships only `python3`/`python3.13` and never populates
`/usr/local/python/bin` (there is no `uv python install` step despite
`UV_PYTHON_INSTALL_DIR`). With no `python` on `PATH`, `poetry install` dies with
`FileNotFoundError: 'python'`, `_cont_start.sh` (`set -e`) exits, and the
container crash-loops **before the server ever starts**.

The Dockerfile fixes this with `ln -sf /usr/bin/python3 /usr/local/bin/python`.
If you ever see Poetry fall back to an in-project `.venv` or complain about
`'python'`, check that symlink first — a missing `python` (not a bad
`POETRY_VIRTUALENVS_PATH`) is the usual cause.

## Running it

- Server boots via `poetry run crack-server` (see `_docker/_cont_start.sh`).
- Tests: `poetry run python -m pytest` from this directory.
- Never commit a `.venv/` here; if one appears, `python` was missing when
  Poetry ran — delete it, fix `python`, re-install.
