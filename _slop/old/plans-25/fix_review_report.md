# Fix review report — segments 1–6

Reviewed the staged (`git add`) implementation of `fix_1`…`fix_6` against the plans, ran the unit
suite, cross-checked the RPC protocol against pi's shipped `docs/rpc.md`, and applied a few cleanups.

**Unit tests: `178 passed` (before and after my cleanups).**
Command: `docker exec crack-dev bash -lc 'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'`

**Live/UI behavior was NOT exercised** (per instruction). Everything that requires driving a real
chat is consolidated into `fix_test_plan.md`; the driver writes results to `fix_test_report.md`.

---

## Per-segment verdict

| Seg | Area | Verdict | Notes |
|-----|------|---------|-------|
| 1 | Seed sandbox index from `base_tree` before `git add -A` | ✅ Correct | `read-tree` added to **both** `_produce_diff` and `_produce_diff_sync`, ordered before staging, RuntimeError on failure. New test asserts read-tree precedes add-A and that an untouched tracked file is not a deletion. |
| 2 | Apply-failure must not re-enqueue | ✅ Correct | `enqueue_chat_apply_failure` → `record_chat_apply_failure` (sets `error`/`error_detail`, `phase=idle`, no `pending`/exchange/job). Both call sites updated (`finalize_chat_sandbox` + `notify_parent_apply_failure` chat branch). `enqueue_chat_apply_failure` deleted; `format_apply_failure` kept (still used by the `run` branch). 3 new tests. |
| 3 | Durable STOP | ✅ Correct | All 5 automatic `stop_requested=False` clears removed (`chat_engine._finish`, `patch.enqueue_chat_system_message`, `patch.finalize_chat_sandbox._bump`, `chats._merge_child_inbox`, `steprun.record_chat_errors`). Human-resume clears kept (`post_message`, `answer_chat_question`) and sub-agent retry clears only the run. `test_stop_durable.py` covers each path incl. sub-agent parity. |
| 4 | Interleave error rows by time | ✅ Correct | `_row_epoch` helper + carry-forward merge replaces `out.extend(error_rows)`. `timestamp` passthrough is safe: `meta` never carries a `timestamp` key (only original/label/compiled/template/media), so the explicit key cannot clobber a real value. Ordering test added. |
| 5 | RPC runner | ✅ Correct | `pi_rpc.arun_agent_hop_rpc` drives `--mode rpc`, one prompt on the wire, authoritative `agent_settled`, clean `abort` on stop/time-cap/sentinel/swap, `_TurnAccumulator` reused, stderr ring buffer. `exec_in` gained `interactive`/`stdin`. Fake RPC subprocess + tests. |
| 6 | pi-owned retries, exact errors, RPC default, delete json machinery | ✅ Correct | `set_auto_retry` on each hop + `retry` block in `.pi/settings.json`. Exact errors mapped (`auto_retry_end.finalError`, `message_update` error, `response success:false`, process-exit → stderr tail) into `PiError(detail=…)`. Python loop shrunk to an infra-only safety net (`RPC_SAFETY_MAX_ATTEMPTS=3`); genuine pi failures surface immediately. RPC is the default; `CRACK_PI_JSON=1` is now a hard error (see note below). Dead json agent-hop machinery removed from `pi_proc.py`; `_TurnAccumulator`/`PiError`/`kill_pid_file`/`arun_pi_text` kept. |

Protocol cross-check against `docs/rpc.md`: `agent_settled`, `set_auto_retry`, `auto_retry_end`/
`finalError`, `willRetry`, `get_state`/`messageCount`, `abort`, `--session-id`/`--session-dir` all
match. `--session-id <id>` is documented as "creating it if missing", which is what makes the
reload-resume path safe (a fresh RPC process on the same id/dir reopens the persisted session).

---

## Cleanups I applied (bad/convoluted implementation → fixed)

1. **`pi_rpc.py`: removed unreachable `get_state` resume block.** `arun_agent_hop_rpc` already
   rewrites `attempt_message → RESUME_MESSAGE` unconditionally whenever `resume_session` (or a safety
   retry) is active, so the inner `if resume_session and prompt_message != RESUME_MESSAGE:` guard in
   `_run_single_rpc_attempt` was **dead code** — never true on any path. Removed the block and the now
   unused `resume_session` parameter of `_run_single_rpc_attempt` (and its call-site argument). The
   tested, conservative behavior is unchanged: on a reload-resume or an infra safety retry the hop
   sends `RESUME_MESSAGE` (pi reopens the same `--session-id`), never replaying the original user
   message. `get_state` is left in the fake as protocol documentation.
2. **`worker.py`: dropped now-unused `import signal` and the `DETACHED_HOP_GRACE_SECONDS` constant**
   (only referenced by the deleted detached-hop reattach block). `recover_detached_hops` now simply
   reaps orphan pid files via `pi_runner.kill_pid_file`, which already unlinks the pid file, so the
   removed manifest/output cleanup is moot.
3. **`paths.py`: deleted dead `hop_manifest_path` / `hop_output_path`** — no remaining references
   after the json machinery was removed.
4. **`pi_proc.py`: corrected the module docstring.** It claimed `CRACK_PI_JSON=1` "forces the removed
   json-mode path"; it actually raises `RuntimeError`. Docstring now says so.

All four are behavior-preserving; suite still `178 passed`.

---

## Intentional deviation from the plan (acceptable)

- **fix_6 step 5 wanted a `CRACK_PI_JSON=1` kill-switch that forces the old path _and_ deletion of the
  json machinery.** Those are mutually exclusive; the implementation deleted the json agent-hop path
  and turned `CRACK_PI_JSON=1` into an explicit hard error (`RuntimeError`) instead of a silent
  fallback. This is the right call — a kill-switch to deleted code cannot work, and a loud error beats
  a confusing no-op. `_docker/run.sh` correctly sets **no** `CRACK_PI_*` env (RPC is the default).

## Things that can only be confirmed live (deferred to `fix_test_plan.md`)

- **Reload-resume does not duplicate the user prompt.** Unit tests cover the safety-net replay
  (`prompt(2) == RESUME_MESSAGE` in `test_crash_retry`/`test_plan41`) and the fake reopens by id, but
  the real "kill the server mid-exchange, restart, confirm the exchange continues from the persisted
  turn without re-sending the original message and with a single session file" needs a live run.
- **Exact provider error reaches the chat banner** (e.g. a real `529 overloaded_error`) — the mapping
  is unit-tested against the fake; a forced live upstream failure confirms the end-to-end banner text.
- **STOP latches across the live worker loop** and only a human message resumes it.
- **fix_1 end-to-end**: a real single-file chat produces a patch with zero `_data/*.bytes` deletions
  and no `patch_apply` follow-up exchange.

No blocking issues found. The staged diff is a faithful, and in places cleaner, implementation of the
six plans.
