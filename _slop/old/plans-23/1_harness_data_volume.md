# Plan 1 — Move all harness state onto a shared `crack-harness-data` volume

> Read `0_overview.md` first. This is the keystone: it makes overlay lowers stable AND
> keeps harness state out of every git patch. Do this cleanly; everything else builds on it.

## Why

The server continuously writes chat state, run state, sub-agent **session dirs**, the
queue, and the worker lock under `.pi/crack/**` — which is *inside the repo tree*, i.e. the
future overlay lower. Writing a lowerdir while sandboxes have it mounted is undefined
behavior. Move all of that mutable state onto a dedicated volume mounted at
`/crack-harness-data` in every container. Persona config (`.pi/crack/sub_agents/`) is
**committed code, read-only at runtime — it stays in `/workspace`.**

## What moves vs. what stays

| Path helper (paths.py) | Today | After |
|---|---|---|
| `harness_dir()` (queue, worker.lock, models_list.json) | `<repo>/.pi/crack/harness` | `/crack-harness-data/harness` |
| `unscripted_chats_dir()` (chats, run states, sessions, media, reports, hop I/O) | `<repo>/.pi/crack/unscripted_chats` | `/crack-harness-data/unscripted_chats` |
| `sub_agents_dir()` (persona config) | `<repo>/.pi/crack/sub_agents` | **unchanged** |
| `templates_dir()`, `project_root()` | repo | **unchanged** |

## Implementation

### 1a. paths.py — introduce a data root

Add near `project_root()`:

```python
def harness_data_root(root: Path | None = None) -> Path:
    """Root for all MUTABLE harness state (chats, runs, sessions, queue, hop I/O).

    Tests pass an explicit ``root`` and get a co-located tree under it. In the
    container, CRACK_HARNESS_DATA_DIR points at the shared /crack-harness-data
    volume. Local dev with neither falls back to the in-repo path (old behavior)."""
    if root is not None:
        return root / ".pi" / "crack"
    env = os.environ.get("CRACK_HARNESS_DATA_DIR")
    if env:
        return Path(env)
    return project_root() / ".pi" / "crack"
```

Repoint the two base dirs to it (keep the `root` param threading intact for tests):

```python
def harness_dir(root: Path | None = None) -> Path:
    return harness_data_root(root) / "harness"

def unscripted_chats_dir(root: Path | None = None) -> Path:
    return harness_data_root(root) / "unscripted_chats"
```

Leave `sub_agents_dir()` exactly as-is (still `(root or project_root()) / ".pi" / "crack" / "sub_agents"`).

**Grep for other hardcoded `.pi/crack` references** that assume co-location and fix any that
touch mutable state:
```bash
docker exec crack-dev bash -exc 'cd /workspace && rg -n "\.pi/crack|unscripted_chats|/harness|CRACK_PI_PROJECT_ROOT" .pi/crack/server/src | rg -v "sub_agents"'
```
Pay attention to `pi_proc.py` hop paths, `queue.py`, `state.py`, `worker.py`, and the
extension's `findSubAgentsDir()` (that one is persona config → leave it).

### 1b. run.sh — create the volume + anchor container, mount into crack-dev

Before the `docker run -d --name crack-dev` block:

```bash
# Shared, non-overlaid volume that holds ALL mutable harness state. It is mounted
# read-write into crack-dev AND (later plans) into every sandbox, so the server and
# the sandboxed pi processes share one stable view that is never an overlay lower.
if ! docker volume ls | grep -q crack-harness-data; then
    docker volume create crack-harness-data
fi
# Anchor container: keeps the volume referenced and gives a stable target for
# inspection/backup (`docker exec crack-harness-data ls /crack-harness-data`).
if ! docker ps -a --format '{{.Names}}' | grep -qx crack-harness-data; then
    docker run -d --name crack-harness-data --restart unless-stopped \
        -v crack-harness-data:/crack-harness-data \
        "$IMG_NAME" sleep infinity
fi
```

Add to the crack-dev `docker run` args:
```bash
  -v "crack-harness-data:/crack-harness-data" \
  -e "CRACK_HARNESS_DATA_DIR=/crack-harness-data" \
```

### 1c. _cont_start.sh — ensure dirs + one-time migration

Near the top (after `export CRACK_PI_PROJECT_ROOT=/workspace`), add:

```bash
export CRACK_HARNESS_DATA_DIR=/crack-harness-data
mkdir -p "$CRACK_HARNESS_DATA_DIR/harness" "$CRACK_HARNESS_DATA_DIR/unscripted_chats"
# One-time migration: if legacy in-repo state exists and the volume is empty, move it.
LEGACY=/workspace/.pi/crack
if [ -d "$LEGACY/unscripted_chats" ] && [ -z "$(ls -A "$CRACK_HARNESS_DATA_DIR/unscripted_chats" 2>/dev/null)" ]; then
    echo "[migrate] copying legacy harness state onto crack-harness-data volume"
    cp -a "$LEGACY/harness/." "$CRACK_HARNESS_DATA_DIR/harness/" 2>/dev/null || true
    cp -a "$LEGACY/unscripted_chats/." "$CRACK_HARNESS_DATA_DIR/unscripted_chats/" 2>/dev/null || true
fi
```

(Leave the legacy dirs in place — don't delete; the copy is a safety net, not a move.)

### 1d. Dockerfile — default env (optional but tidy)

Add before `VOLUME /root`: `ENV CRACK_HARNESS_DATA_DIR=/crack-harness-data`.

## Verification

1. **Rebuild + boot:**
   ```bash
   cd /home/p/VIDOEGAME/crack/_docker && ./build.sh && ./run.sh && sleep 5
   docker exec crack-dev bash -exc 'ls -la /crack-harness-data/ /crack-harness-data/unscripted_chats/ | head'
   ```
2. **Server healthy:** `docker exec crack-dev bash -exc 'curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:9847/'` → `200`.
3. **Unit tests still green** (they pass explicit `root=`, so must be unaffected):
   ```bash
   docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && uv run pytest -q tests/test_state.py tests/test_sub_agents.py tests/test_async_worker.py'
   ```
4. **Nemotron sample chat** (see `0_overview.md`), then confirm the new chat landed on the
   **volume** and NOT in the repo tree:
   ```bash
   docker exec crack-dev bash -exc '
     ls /crack-harness-data/unscripted_chats/ | tail -3
     # nothing new should be written under the repo tree:
     find /workspace/.pi/crack/unscripted_chats -newermt "-5 minutes" 2>/dev/null | head
   '
   ```
   The chat dir must appear under `/crack-harness-data/...`; the `find` under `/workspace`
   must be empty (bar pre-existing legacy files).
5. **Isolation from git:** `docker exec crack-dev bash -exc 'cd /workspace && git status --short | rg unscripted_chats || echo "clean: harness state not in git"'`.

## Gotchas

- Tests import `crack_server` — make sure `harness_data_root(None)` still falls back to the
  repo path when `CRACK_HARNESS_DATA_DIR` is unset, so non-container test runs don't scatter
  files into `/crack-harness-data`.
- The `find_run_dir()` glob walks `unscripted_chats_dir()` — it inherits the new root
  automatically. Verify a sub-agent chat (spawn) still resolves run dirs (part of test 3).
- If migration copy is slow/huge, that's fine once; subsequent boots skip it (volume non-empty).

## Report

Write `_slop/report-23/1_harness_data_volume.md`: the diff summary, grep results for stray
`.pi/crack` writers you fixed or intentionally left, test output, the sample chat id + its
path on the volume, and confirmation that `/workspace/.pi/crack/unscripted_chats` received
no new writes during the run.
