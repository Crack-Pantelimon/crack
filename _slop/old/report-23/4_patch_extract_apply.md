# Plan 4 report — Baseline-diff patch extraction, 95 MB guard, and auto-apply

## Summary

Each sandboxed conversation now snapshots a baseline `git write-tree` at session start,
diffs against it at end (`git diff <base> <end>`), and writes `patch.diff` under the
conversation's harness dir. Non-empty patches auto-apply: sub-agents → parent overlay via
`podman exec … git apply --3way`; top-level chats → crack-dev host `/workspace`. A 95 MB
(decimal) staged-file guard nags the agent up to five times, then excludes oversized paths.

## Byte threshold

**95 × 10⁶ bytes (95.0 MB decimal)**, not MiB. Constant: `patch.MAX_FILE_BYTES = 95 * 1_000_000`.

## Files changed

| File | Change |
|------|--------|
| `patch.py` | **New** — baseline capture, guarded extraction, apply, nag/conflict messaging |
| `chats.py` | Baseline at `run_chat` start; `finalize_chat_sandbox` on idle/stop; stop no longer destroys sandbox early |
| `sub_agents/runner.py` | `finish()` calls `finalize_run_sandbox` before parent handoff |
| `sub_agents/base.py` | `ensure_baseline` per run; `patch_nag` / `patch_conflict` hop messages; cascade stop extracts without apply |
| `tests/test_patch.py` | **New** — unit tests for baseline, nag, message formatting |

## Code path

### Session start

- **Chat:** `run_chat` → `ensure_sandbox(chat_id)` → `patch.capture_baseline(sandbox, chat_dir)` (overwrites each job).
- **Sub-agent:** first hop → `ensure_baseline(sandbox, run_dir)` (once per run; `base_tree` kept until finalize).

### Session end (before `destroy_sandbox`)

1. Read `base_tree` from artifact dir.
2. `git add -A` in sandbox; list staged paths > 95 MB.
3. If oversized and not forceful and `patch_guard_attempts < 4`: `git reset`, nag agent, return (no destroy).
4. On 5th attempt or `forceful=True`: `git add -A`, `git reset -- <big…>`, `write-tree`, `git diff base end` → `patch.diff`.
5. Empty diff → skip apply, no nag.
6. **Apply:** host (`apply_patch_on_host`) for chats; `podman exec <parent_sbx> git apply --3way` for sub-agents (`--reject` fallback → conflict).
7. On apply failure: enqueue conflict message (see below); **do not roll back**.
8. Unlink `base_tree`, destroy sandbox.

### Nag sources

- Chat: `pending` message, `source=patch_guard`, re-enqueues `CHAT_JOB_SLUG`.
- Sub-agent: `enqueue_step("run", {patch_nag: …})`, phase back to `running`.

## Conflict / apply-failure message (shipped wording)

```
Patch application failed.

git apply stderr:
<stderr>

The full patch is at: /crack-harness-data/.../patch.diff

Resolve the conflict directly in the working tree, finish applying the patch, then continue your task. The full patch is at <same path> for reference.
```

Implemented in `patch.format_apply_failure()`.

## Big-file nag message (shipped wording)

```
The harness detected file(s) larger than 95 MB staged for the patch. They cannot be included. Please add them to `.gitignore` or delete them, then stop.

- /workspace/big.bin (120000000 bytes)
```

Implemented in `patch.format_big_file_nag()`.

## Commands run

```bash
docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && uv run pytest tests/test_patch.py tests/test_sandbox.py -q'
# 15 passed

docker exec crack-dev bash -exc 'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. uv run pytest -q tests/ --ignore=tests/test_vision_media.py'
# 134 passed in 47s
```

## Verification results

### 1. Delta correctness (baseline diff, not naive dirty set)

- **Chat id:** `1784721588717`
- **Patch:** `/crack-harness-data/unscripted_chats/1784721588717/patch.diff`
- **Result:** `grep -c "^diff --git"` → **1** (host had ~10 pre-existing dirty files; patch contains only `_slop/report-23/README.md`).
- **Trajectory:** `/crack-harness-data/unscripted_chats/1784721588717/`

### 2. Top-level apply to host

- **Same chat:** `1784721588717`
- **Result:** `PATCH_VERIFY_ONE` present in host `/workspace/_slop/report-23/README.md` after idle (auto-applied from sandbox overlay).

### 3. No-change task

- **Chat id:** `1784721632560`
- **Patch:** `/crack-harness-data/unscripted_chats/1784721632560/patch.diff` (0 bytes)
- **Result:** phase `idle`, no `patch_guard` / system nag in `pending` or exchanges.

### 4. Big-file guard

- **Nemotron transcript chat id:** `1784721772892`
- **Trajectory:** `/crack-harness-data/unscripted_chats/1784721772892/`
- **Result:** After creating `big.bin` (120 MB), first finalize nag appeared in exchanges listing `/workspace/big.bin (120000000 bytes)`; `patch_guard_attempts=1`. (Full 5-cycle nemotron run not waited out — see programmatic check below.)
- **Programmatic sandbox conv:** `patchtest_bigfile2` — attempts 0–3 `needs_nag=True`; attempt 4 produces patch containing `README.md` edit, **excludes** `big.bin`.

### 5. Sub-agent auto-apply + conflict

- **Apply success (scripted):** child sandbox `patchtest_child_ok` → parent `patchtest_parent_ok`; `CHILD_OK` visible in parent overlay file.
- **Conflict (scripted):** child `patchtest_child` vs parent `patchtest_parent` on `_slop/report-23/conflict_test.txt`; `git apply --3way` and `--reject` both failed; parent left dirty with `.rej` file and `PARENT` content retained (not rolled back). Conflict stderr matches message template above.

**Not E2E-tested:** full nemotron sub-agent spawn → `wait_join` → parent overlay apply (scripted podman path matches `finalize_run_sandbox` production code). Conflict message delivery to parent **chat** via `enqueue_chat_system_message` not exercised live; sub-agent `patch_conflict` hop wiring is in `base._compile_message`.

## Notes for Plan 5+

- `base_tree` is removed after each finalize; each new `run_chat` job recaptures baseline.
- Chat stop (`stop_requested`) uses `forceful=True` finalize (single pass, big files excluded).
- Cascade sub-agent stop calls `finalize_run_sandbox(apply_to_parent=False)` so stopped children don't mutate parent mid-stop.
- Sub-agent nag reopens run (`phase=running`) before `parent_notified`; parent handoff waits until patch guard clears.
- `patch.diff` persists on the harness volume for post-mortem; overlay upper remains under `/crack-harness-data/overlays/<id>/upper`.
