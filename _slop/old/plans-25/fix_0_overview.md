# Fix plan overview — pi chat failure cascade

Investigation of chat `http://localhost:9847/chats/1784739382358` found **four independent root
causes** stacked into a self-sustaining failure cascade (endless "Patch application failed"
re-prompts, duplicated user prompts, opaque `pi crashed mid-turn` errors, STOP that won't stick).

This directory breaks the fix into **ordered, self-contained segments**. Each `fix_N_*.md` is
written to be implemented on its own by a smaller model (≤250k context) that has NOT seen this
investigation. Implement them **in numeric order**, rebuild/restart, verify the segment's checks,
then move to the next. Come back to the lead session to confirm each fix after the fact.

## Root-cause map → segment

| # | Root cause | Segment | Risk |
|---|-----------|---------|------|
| 1 | **The fuel.** Every extracted patch spuriously "deletes" 148 tracked-but-gitignored `_data/**/*.bytes` files, because the sandbox's frozen-base git repo is `git init`'d with an **empty index**, so `git add -A` treats those tracked cache files as ignored and drops them from `end_tree`. Host `git apply` then fails deterministically. | `fix_1_overlay_materialization.md` | low |
| 2 | **The engine.** On a host `git apply` failure, `finalize_chat_sandbox` **enqueues a brand-new agent turn** whose user prompt is the 60 KB apply-failure stderr. That turn makes another bad patch → fails again → loops forever. | `fix_2_stop_patch_apply_loop.md` | low |
| 3 | **STOP is not durable.** `stop_requested` is cleared in ~5 places (every exchange end, every enqueue), so repeated STOP clicks are overwritten within ~1s and the chat restarts itself. | `fix_3_durable_stop.md` | med |
| 4 | **Errors are dumped at the bottom on refresh, out of time order.** `merge_exchange_sidecars` appends all error rows after the whole trajectory instead of interleaving by timestamp. | `fix_4_trajectory_error_order.md` | low |
| — | **Control plane.** pi is driven by `pi --mode json` as a fire-and-forget subprocess whose stdout is tailed and crashes are *inferred*, which causes false "crashed" classifications, duplicate session files, replayed prompts, and useless error details. Move to `pi --mode rpc` (authoritative `agent_settled`/`abort`/exact errors) and delegate LLM retries to pi itself. | `fix_5_pi_rpc_runner.md`, `fix_6_pi_rpc_retries_errors.md` | high |

**Segments 1–4 are Python-only and already stop the live cascade** — they are the shippable fix.
**Segments 5–6 (RPC) are the larger refactor** that dissolves the false-crash/duplicate-session/
opaque-error class; do them after 1–4 are verified.

## Shared environment & workflow (every segment repeats this)

- Host repo root: `/home/p/VIDOEGAME/crack`. Server code: `.pi/crack/server/src/crack_server/`.
- The repo is **bind-mounted** into the `crack-dev` container at `/workspace` (same files, live).
  Editing on the host is instantly visible in the container.
- The server runs `poetry run crack-server` (FastAPI/uvicorn + in-process queue worker). It does
  **not** auto-reload — you must restart it to load Python edits.
- **Rebuild / restart** (from the host):
  ```bash
  cd /home/p/VIDOEGAME/crack/_docker && ./build.sh && ./run.sh
  ```
  `./build.sh` rebuilds the image (only needed when the Dockerfile or dependencies change).
  `./run.sh` recreates and restarts the `crack-dev` container and is what loads your Python
  edits (harness data persists in the `crack-harness-data` volume). **For pure-Python edits,
  `./run.sh` alone is enough** — you can skip `./build.sh`.
- **Run the server test suite** (inside the container; `python` is not on PATH — use poetry):
  ```bash
  docker exec crack-dev bash -lc \
    'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
  ```
  (172 tests pass today. Keep them green.)
- **Tooling lives inside the container**, not the host: `pi`, `git`, `podman`, `rg`, `jq`.
  Use `docker exec crack-dev <cmd>` for the server / sandbox-host, and
  `docker exec crack-sbx-<chatId> <cmd>` for a specific chat's sandbox
  (container name = `crack-sbx-<conversation id>`).
- App UI: http://localhost:9847 . A chat: http://localhost:9847/chats/<id> .
  Chat state on disk: `/crack-harness-data/unscripted_chats/<id>/chat.json`.
- pi docs (inside the container image) live under the pi package `docs/` dir; find them with:
  `docker exec crack-dev bash -lc 'ls $(dirname $(dirname $(readlink -f $(which pi))))/*/docs 2>/dev/null || find / -path "*pi-coding-agent/docs/rpc.md" 2>/dev/null'`
- Do **not** commit unless explicitly asked. Keep diffs minimal and match surrounding style.
