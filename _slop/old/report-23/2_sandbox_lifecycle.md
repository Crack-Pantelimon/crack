# Plan 2 report — sandbox lifecycle module

## Summary

Shipped `crack_server/sandbox.py`: async podman helpers to create one long-lived sandbox
container per conversation (`crack-sbx-<id>`), exec commands into it, kill a single pi
session, and destroy the container. Wired `crack-net` so sandboxes reach crack-server at
`http://crack-dev:9847`, and made the pi extension's server host configurable via
`CRACK_PI_HOST`.

Plan 1 confirmed merged: harness state lives on `/crack-harness-data` (`paths.harness_data_root`,
`run.sh` volume mount, `_cont_start.sh` migration).

## Files changed

| File | Change |
|------|--------|
| `.pi/crack/server/src/crack_server/sandbox.py` | **New** — sandbox lifecycle API (see below). |
| `.pi/crack/server/tests/test_sandbox.py` | **New** — 9 unit tests with mocked `_podman`. |
| `_docker/run.sh` | Create `crack-net`; attach `crack-dev` with `--network crack-net --network-alias crack-dev`. |
| `.pi/extensions/crack/index.ts` | `CRACK_PI_HOST` (default `127.0.0.1`) for `BASE` URL. |

## Module API (`crack_server/sandbox.py`)

| Symbol | Purpose |
|--------|---------|
| `sandbox_name(conv_id)` | `crack-sbx-<conv_id>` |
| `harness_volume_host_path()` | Host mountpoint via `podman volume inspect crack-harness-data` |
| `ensure_network()` | Create `crack-net` if missing |
| `ensure_sandbox(conv_id)` | Idempotent create/start; CoW overlays under harness volume |
| `exec_in(name, argv, *, env, cwd, detached, stdout, stderr)` | Launch `podman exec`; returns `asyncio.subprocess.Process` |
| `kill_session(name, session_id)` | `pkill -TERM` then grace wait then `pkill -KILL` inside sandbox |
| `destroy_sandbox(conv_id)` | `podman kill` + `rm -f` |

Constants: `CRACK_NET`, `HARNESS_VOLUME`, `SANDBOX_IMAGE` (`localhost/crack-dev:latest`).

### Resolved host paths (for Plan 3)

| What | Path |
|------|------|
| Host repo (overlay lower) | `$CRACK_HOST_REPO_ROOT` → `/home/p/VIDOEGAME/crack` (from `run.sh`) |
| Harness volume host mount | `/home/p/.local/share/containers/storage/volumes/crack-harness-data/_data` |
| Overlay per conv `<id>` | `<harness_mount>/overlays/<id>/{upper,work}` (also visible in-container at `/crack-harness-data/overlays/<id>/`) |
| In-container workspace | `/workspace` (CoW over host repo) |
| Shared harness I/O | `/crack-harness-data` (plain bind, not overlaid) |
| crack-server from sandbox | `http://crack-dev:9847` (`CRACK_PI_HOST=crack-dev` set at sandbox create) |

Sandbox `podman run` mounts:

```
-v $CRACK_HOST_REPO_ROOT:/workspace:O,upperdir=<vol>/overlays/<id>/upper,workdir=<vol>/overlays/<id>/work
-v crack-dev-target-dir:/workspace/target:O
-v crack-dev-root-dir:/root:O
-v crack-harness-data:/crack-harness-data
--network crack-net
```

## Podman version note

crack-dev runs **podman client 5.4.2** (Debian package) against the **host podman 6.0.1**
daemon over `CONTAINER_HOST=unix:///run/podman/podman.sock`. Integration verification
succeeded with this client/server skew; no workarounds needed beyond using the remote socket.

## Commands run

### Unit tests

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && uv run pytest tests/test_sandbox.py -q'
# 9 passed in 0.13s
```

### Integration verification (plan script, with stdout=PIPE on exec_in)

```bash
docker exec crack-dev bash -exc '...'  # full script in plan 2
```

Output:

```
started crack-sbx-9999999999999_deadbeef
pi rc 0 out 0.80.10
pi ok
curl 200
harness_vol_host /home/p/.local/share/containers/storage/volumes/crack-harness-data/_data
OK host isolated
hi
OK no leaked sandbox
crack-net
```

Checks: pi reachable in sandbox; workspace overlay isolated (no `SANDBOX` in host
`_docker/README.md`); curl to `crack-dev:9847` returned `200`; probe file on shared volume
visible from crack-dev; no leaked `crack-sbx-*` after `destroy_sandbox`.

### Network confirm

```bash
docker network inspect crack-net --format '{{range .Containers}}{{.Name}} {{end}}'
# crack-dev
```

crack-dev uses `crack-net` (not `--network host`).

### Container restart

```bash
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh
```

## Not done in this plan

- No wiring of `pi_proc.arun_agent_hop` through sandboxes (Plan 3).
- No sample nemotron chat E2E (Plan 2 scope is the module + verification above only).

## Notes for Plan 3

- `exec_in` returns a bare `asyncio.subprocess.Process` — caller must pass `stdout`/`stderr`
  pipes or redirect to hop output files.
- `ensure_sandbox` is safe to call on every hop; existing containers get `podman start`.
- Always `destroy_sandbox` when a conversation ends; leaked `sleep infinity` containers pin
  overlay upper/work dirs.
- Extension in sandbox: `CRACK_PI_HOST=crack-dev` is set by `ensure_sandbox`; crack-dev
  itself keeps `CRACK_PI_HOST=0.0.0.0` for uvicorn bind only (extension not loaded there).
