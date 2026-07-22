# Plan 1 report — harness data volume

## Summary

Moved all mutable harness state (chats, runs, sessions, queue, worker lock, hop I/O,
MCP HTTP logs) off the repo bind-mount onto a dedicated Docker volume
`crack-harness-data` mounted at `/crack-harness-data` in `crack-dev`. Persona config
(`.pi/crack/sub_agents/`) stays in `/workspace` unchanged.

## Files changed

| File | Change |
|------|--------|
| `.pi/crack/server/src/crack_server/paths.py` | Added `harness_data_root()`; repointed `harness_dir()` and `unscripted_chats_dir()` through it. Added test-isolation guard: when `CRACK_PI_PROJECT_ROOT` is not `/workspace`, harness state stays co-located under the test tmp dir even if `CRACK_HARNESS_DATA_DIR` is set. |
| `_docker/run.sh` | Create `crack-harness-data` volume + anchor container; mount volume into `crack-dev` with `CRACK_HARNESS_DATA_DIR=/crack-harness-data`. |
| `_docker/_cont_start.sh` | Export `CRACK_HARNESS_DATA_DIR`, mkdir harness dirs on volume, one-time legacy migration copy, MCP HTTP log paths moved to volume. |
| `_docker/Dockerfile` | `ENV CRACK_HARNESS_DATA_DIR=/crack-harness-data` before `VOLUME /root`. |

## Grep for stray `.pi/crack` writers

```bash
docker exec crack-dev bash -exc 'cd /workspace && rg -n "\.pi/crack|unscripted_chats|/harness|CRACK_PI_PROJECT_ROOT" .pi/crack/server/src | rg -v "sub_agents"'
```

**Intentionally left (docstrings / UI only — no path construction):**
- `chats.py:3` — module docstring (stale path text; behavior now uses `harness_data_root`)
- `queue.py:3`, `settings.py:2`, `vision.py:5` — docstrings referencing old layout
- `routes_sub_agents.py:418` — HTML help text for persona dir (correct: stays in repo)
- `sub_agents/registry.py:1` — docstring for persona discovery (correct: stays in repo)

**Already correct (use path helpers, inherit new root automatically):**
- `worker.py` — `paths.unscripted_chats_dir()`
- `pi_proc.py`, `state.py`, `queue.py` (runtime) — no hardcoded harness paths
- `.pi/extensions/crack/index.ts` — `findSubAgentsDir()` points at `.pi/crack/sub_agents` (persona config, unchanged)

**Fixed outside `paths.py`:**
- `_cont_start.sh` — MCP HTTP / Xvfb logs were hardcoded to `/workspace/.pi/crack/harness/mcp-http/`; now under `$CRACK_HARNESS_DATA_DIR/harness/mcp-http/`

## Commands run

```bash
cd /home/p/VIDOEGAME/crack/_docker && ./build.sh && ./run.sh && sleep 5
docker exec crack-dev bash -exc 'ls -la /crack-harness-data/ /crack-harness-data/unscripted_chats/ | head'
docker exec crack-dev bash -exc 'curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:9847/'
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. uv run pytest -q tests/test_state.py tests/test_sub_agents.py tests/test_async_worker.py'
# nemotron sample chat (see 0_overview.md recipe)
```

## Verification results

### 1. Rebuild + boot
Volume created; legacy state migrated on first boot:
```
/crack-harness-data/harness
/crack-harness-data/unscripted_chats   (pre-existing chats from migration copy)
```

### 2. Server healthy
```
200
```

### 3. Unit tests
First run without `PYTHONPATH=tests:.` hit collection errors (`ModuleNotFoundError: tests`).
With `PYTHONPATH=tests:.` (needed pre-existing for these test modules):

```
28 passed in 12.45s
```

Note: first test attempt before the test-isolation fix in `harness_data_root` failed 4 tests
because `CRACK_HARNESS_DATA_DIR` caused pytest tmp_path chats to land on the shared volume
instead of isolated tmp dirs. Fixed by honouring non-`/workspace` `CRACK_PI_PROJECT_ROOT`.

### 4. Nemotron sample chat

- **Chat id:** `1784719085501`
- **Trajectory path:** `/crack-harness-data/unscripted_chats/1784719085501/`
- **Phase:** reached `idle` on poll 5 (~25s)
- **Task outcome:** `/workspace/HELLO_SANDBOX.txt` contains `PONG`
- **Repo tree new writes:** `find /workspace/.pi/crack/unscripted_chats -newermt "-10 minutes"` → **empty**

### 5. Git isolation
```
clean: harness state not in git
```

## Notes for Plan 2+

- `crack-harness-data` anchor container (`docker exec crack-harness-data ls /crack-harness-data`) is running alongside `crack-dev`.
- Sandboxes should mount the same volume read-write with `-e CRACK_HARNESS_DATA_DIR=/crack-harness-data` (as sketched in plans 2/3).
- Legacy in-repo dirs under `/workspace/.pi/crack/{harness,unscripted_chats}` were **copied, not deleted** — safe to ignore; server no longer writes there.
- A few stray chat dirs from the pre-fix failed test run landed on the volume (`1784719013329_*`); harmless.
