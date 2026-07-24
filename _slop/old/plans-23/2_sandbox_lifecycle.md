# Plan 2 — Sandbox lifecycle module + shared network + host-path plumbing

> Read `0_overview.md` and confirm Plan 1 is merged (state lives on `/crack-harness-data`).
> This plan builds the reusable machinery to create/exec/kill sandbox containers. It does
> NOT yet route real pi hops through them (that's Plan 3) — it delivers a tested module.

## Deliverable

A new module `crack_server/sandbox.py` with a small, well-tested API for managing
per-conversation sandbox containers, plus the `crack-net` network and the extension change
that lets a sandboxed pi reach crack-server by hostname.

## Concepts

- **One sandbox per conversation** (chat or run), named `crack-sbx-<id>` where `<id>` is the
  chat_id or run_id (podman names allow `[a-zA-Z0-9_.-]`; chat/run ids are digits+`_`+hex,
  safe). It runs `sleep infinity`; hops are `podman exec`'d into it.
- **Mounts** (all via HOST paths — you are driving host podman):
  - `-v <HOST_REPO>:/workspace:O,upperdir=<OVL>/upper,workdir=<OVL>/work` — CoW workspace.
  - `-v crack-dev-target-dir:/workspace/target:O` — CoW Rust target.
  - `-v crack-dev-root-dir:/root:O` — CoW home (browser caches etc. shared read-only in lower).
  - `-v crack-harness-data:/crack-harness-data` — **shared, NOT overlaid** (hop I/O, sessions,
    state). This is the channel crack-dev and the sandbox both read/write.
  - `--network crack-net` — reach crack-server as `http://crack-dev:9847`.
  - `<OVL>` lives on the shared volume so crack-dev can inspect it after a crash:
    `/crack-harness-data/overlays/<id>/{upper,work}` (create both before `podman run`).
- **Path translation:** crack-dev sees the repo at `/workspace`; host podman needs the HOST
  path `os.environ["CRACK_HOST_REPO_ROOT"]`. The overlay dirs are on a named volume, so pass
  them by their **in-container** path? NO — `upperdir`/`workdir` are host-podman paths on the
  volume's host mountpoint. Resolve the volume's host path once via
  `podman volume inspect crack-harness-data --format '{{.Mountpoint}}'` and build
  `<mountpoint>/overlays/<id>/{upper,work}`. Store that helper in the module.

## Implementation: `crack_server/sandbox.py`

Sketch (async, using `asyncio.create_subprocess_exec` like pi_proc.py):

```python
CRACK_NET = "crack-net"
SANDBOX_IMAGE = "localhost/crack-dev:latest"

def sandbox_name(conv_id: str) -> str:
    return f"crack-sbx-{conv_id}"

def _host_repo() -> str:
    return os.environ["CRACK_HOST_REPO_ROOT"]  # set by run.sh

async def _podman(*args, timeout=60) -> tuple[int, str, str]:
    """Run one podman command against the host socket; return (rc, out, err)."""
    ...

async def harness_volume_host_path() -> str:
    rc, out, _ = await _podman("volume", "inspect", "crack-harness-data",
                               "--format", "{{.Mountpoint}}")
    return out.strip()

async def ensure_network() -> None:
    rc, *_ = await _podman("network", "exists", CRACK_NET)
    if rc != 0:
        await _podman("network", "create", CRACK_NET)

async def ensure_sandbox(conv_id: str) -> str:
    """Idempotently create+start the sandbox; return its name. Safe to call every hop."""
    name = sandbox_name(conv_id)
    rc, *_ = await _podman("container", "exists", name)
    if rc == 0:
        # exists — make sure it is running (a stopped one is restarted)
        await _podman("start", name)
        return name
    await ensure_network()
    vol = await harness_volume_host_path()
    ovl = f"{vol}/overlays/{conv_id}"
    os.makedirs(f"/crack-harness-data/overlays/{conv_id}/upper", exist_ok=True)
    os.makedirs(f"/crack-harness-data/overlays/{conv_id}/work", exist_ok=True)
    await _podman(
        "run", "-d", "--name", name, "--network", CRACK_NET,
        "-v", f"{_host_repo()}:/workspace:O,upperdir={ovl}/upper,workdir={ovl}/work",
        "-v", "crack-dev-target-dir:/workspace/target:O",
        "-v", "crack-dev-root-dir:/root:O",
        "-v", "crack-harness-data:/crack-harness-data",
        "-e", "CRACK_HARNESS_DATA_DIR=/crack-harness-data",
        "-e", "CRACK_PI_PROJECT_ROOT=/workspace",
        "-e", "CRACK_PI_HOST=crack-dev",   # extension talks to the server by hostname
        SANDBOX_IMAGE,
        "sleep", "infinity",
        timeout=120,
    )
    return name

async def exec_in(name: str, argv: list[str], *, env=None, cwd="/workspace",
                  detached=False, stdout=None) -> ...:
    """Build a `podman exec [-d] -w cwd -e K=V ... name argv...`. Returns the
    asyncio subprocess (Plan 3 tails its output / the shared file)."""
    ...

async def kill_session(name: str, session_id: str) -> None:
    """Mid-run kill: signal only the pi for one session (SIGTERM then SIGKILL)."""
    await _podman("exec", name, "pkill", "-TERM", "-f", session_id)
    # escalate after a grace period if still alive (see pi_proc kill semantics)

async def destroy_sandbox(conv_id: str) -> None:
    await _podman("kill", sandbox_name(conv_id))
    await _podman("rm", "-f", sandbox_name(conv_id))
```

Keep it dependency-light and mirror pi_proc.py's logging (`logging.getLogger("uvicorn.error")`).

## run.sh + Dockerfile changes

- run.sh: create `crack-net` and attach crack-dev to it. Add before the crack-dev run:
  ```bash
  if ! docker network exists crack-net; then docker network create crack-net; fi
  ```
  Add `--network crack-net --network-alias crack-dev` to crack-dev's `docker run` args so
  sandboxes resolve `crack-dev`. (crack-dev still keeps its published `127.0.0.1` ports for
  the host UI.)
- crack-server already binds `0.0.0.0` in-container (`CRACK_PI_HOST=0.0.0.0` in
  `_cont_start.sh`) — good, sandboxes on `crack-net` can reach `crack-dev:9847`.

## Extension change (`.pi/extensions/crack/index.ts`)

`BASE` is hardcoded to `127.0.0.1`. Make the host configurable so a sandboxed pi reaches
crack-dev:

```ts
const HOST = process.env.CRACK_PI_HOST ?? "127.0.0.1";
const BASE = `http://${HOST}:${process.env.CRACK_PI_PORT ?? "9847"}`;
```

(Locally / in crack-dev it stays `127.0.0.1`; in a sandbox `CRACK_PI_HOST=crack-dev`.)

## Verification (this plan is unit-testable without touching real hops)

Do it all from crack-dev. Use a throwaway conv id like `9999999999999_deadbeef`.

```bash
docker exec crack-dev bash -exc '
  cd /workspace/.pi/crack/server
  uv run python - <<PY
import asyncio, os
from crack_server import sandbox as s
cid="9999999999999_deadbeef"
async def main():
    await s.ensure_network()
    name = await s.ensure_sandbox(cid)
    print("started", name)
    # 1. pi is reachable inside
    p = await s.exec_in(name, ["pi","--version"]); print("pi ok")
    # 2. workspace is overlaid + isolated: write inside, host must stay clean
    await s.exec_in(name, ["bash","-c","echo SANDBOX >> /workspace/_docker/README.md"])
    # 3. sandbox reaches crack-server by hostname over crack-net
    await s.exec_in(name, ["bash","-c","curl -s -o /dev/null -w %{http_code} http://crack-dev:9847/"])
    # 4. shared volume visible both sides
    await s.exec_in(name, ["bash","-c","echo hi > /crack-harness-data/overlays/%s/probe" % cid])
    await s.destroy_sandbox(cid)
asyncio.run(main())
PY
  # host README must NOT contain SANDBOX:
  grep -q SANDBOX /workspace/_docker/README.md && echo "FAIL host mutated" || echo "OK host isolated"
  # probe written to shared volume must be visible from crack-dev:
  cat /crack-harness-data/overlays/9999999999999_deadbeef/probe
  podman ps -a --format "{{.Names}}" | grep crack-sbx || echo "OK no leaked sandbox"
'
```

Expected: `pi ok`; curl returns `200`; `OK host isolated`; `hi` printed; no leaked
`crack-sbx-*` container. Also confirm `--network host` was **not** used (must be `crack-net`):
`docker exec crack-dev bash -exc 'podman network inspect crack-net --format "{{.Name}}"'`.

## Gotchas

- `podman container exists` / `network exists` return nonzero when absent — check rc, don't
  parse text.
- The overlay `work` dir must be on the **same filesystem** as `upper` — both are under the
  same volume mountpoint, so this holds.
- Rootless podman shifts uids in the overlay upper; files are owned by the mapped host user
  and remain readable from crack-dev (verified in prototyping). Don't `chown`.
- Always `destroy_sandbox` in tests — a leaked `sleep infinity` container holds the overlay.

## Report

`_slop/report-23/2_sandbox_lifecycle.md`: the module API you shipped, the exact verification
output, any podman-version quirks (5.4 client → 6.0 host), and the resolved host paths so
Plan 3 knows the layout.
