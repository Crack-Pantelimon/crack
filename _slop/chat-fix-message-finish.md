# Fix real root cause of "chat hangs / work disappears": `agent_end` treated as final even when pi says `willRetry: true`

## Context

Fix A (detail capture) and Fix B (detached-pid ledger/sweep) from the earlier
part of this plan are **already implemented and merged** by the user
(`d1eb872`, `3a7cf3a`) and confirmed correct by re-reading the diffs. They
are not the cause of the new regression ("messages are hanging", "the chat
shouldn't just disappear like that" — chat `1784586114784`).

Live forensics in `crack-dev` (read-only `docker exec`) found the real bug,
and it long predates Fix A/B — it just never showed up on simple chats:

- Chat `1784586114784`'s `agent.hop.json` shows `status: "done"`, `offset:
  232368`, and a `detached_pids` entry for `pid 274`. `ps -p 274` shows the
  pid is **gone** — but `agent.hop.jsonl` is **1,324,912 bytes**, i.e. ~1.09MB
  (590 events) were written **after** the offset our harness stopped reading
  at, and were **never** persisted into `chat.json` (which still shows only
  5 turns, `phase: idle`, no errors).
- Scanning the file for terminal-type events shows **nine** `agent_end`
  events across the one `pi` invocation, at byte offsets 232368, 374463,
  404082, 1181053, 1232409, 1319496, 1321261, 1323026, 1324792 — followed by
  a single `agent_settled` at EOF (1324912). Every `agent_end` except the
  last carries `"willRetry": true`; the last carries `"willRetry": false`.
- **`_process_stream_line`** ([pi_proc.py:682-684](.pi/crack/server/src/crack_server/pi_proc.py#L682-L684)):
  ```python
  if etype in ("agent_end", "agent_settled"):
      sink.terminal = True
      return True
  ```
  treats the **first** `agent_end` as the end of the hop, full stop —
  ignoring `willRetry`. `_tail_events` ([pi_proc.py:736-737](.pi/crack/server/src/crack_server/pi_proc.py#L736-L737))
  returns the instant that happens, at byte 232368 — exactly matching the
  frozen manifest offset. Everything pi does afterward (8 more internal
  agent-loop cycles finishing the real multi-crate/sub-agent orchestration
  work — the actual Cargo.toml edits, the actual docstring passes) is
  written to a file nobody is tailing anymore.
- This is **documented, first-class pi SDK behavior**, not a guess — read
  straight from pi's own shipped type definitions
  (`/usr/lib/node_modules/@earendil-works/pi-coding-agent/dist/core/agent-session.d.ts`,
  read live in the container):
  ```ts
  { type: "agent_end"; messages: AgentMessage[]; willRetry: boolean } |
  { type: "agent_settled" }
  ```
  pi's own `AgentSession` class even has a private field
  `_willRetryAfterAgentEnd` for this exact distinction internally.
  `agent_end` fires once per internal agent-loop segment; `willRetry: true`
  means "I'm continuing automatically, more `agent_end`s (or `agent_settled`)
  are coming" — it is **not** a process-exit signal. Only `agent_settled`
  (or an `agent_end` with `willRetry: false`) means the CLI process is
  actually about to finish.
- Why this never showed up before: every previously-investigated chat
  (including the `-9` bug and the "empty turns" bug) only ever produced a
  **single** `agent_end` per attempt (simple prompt → one tool call → one
  reply), where `willRetry` is always `false`/absent, so "first `agent_end`
  = terminal" happened to be correct. This is the first chat with a prompt
  big enough (30 crates, spawn-and-`wait`-for-sub-agents orchestration) to
  make pi loop internally through multiple agent-loop segments in one CLI
  invocation, which is what exposed the bug.
- Consequence of the interaction with the (correct, already-shipped) `-9`
  fix: since we now correctly *don't* SIGKILL a "terminal" pi, the harness
  quietly detaches after the first `agent_end`, marks the hop `"done"`,
  `chat_engine.run_exchange` sets `phase = "idle"` (one hop per exchange —
  [chat_engine.py](.pi/crack/server/src/crack_server/chat_engine.py)) — and
  the UI shows a clean, finished-looking chat with only the first slice of
  work, while the real `pi` process keeps running standalone, fully
  disconnected, for however long the rest of the task takes, and its output
  is permanently lost (no live "done"-status manifest is ever re-tailed —
  `_live_detached_manifest` ([pi_proc.py:985-1006](.pi/crack/server/src/crack_server/pi_proc.py#L985-L1006))
  only reattaches `status == "running"`). This is exactly "the chat
  shouldn't just disappear like that" — the visible chat looks done/idle,
  the real work vanishes into an orphaned process.

### The podman/overlayfs log spam — unrelated
`overlayfs: ... falling back to xino=off` / `index=off` is a generic,
well-known kernel message about the container storage driver's backing
filesystem not supporting `open_by_handle_at`-style file handles. It's a
storage-layer (image/container layer mount) concern, unrelated to our pi
subprocess plumbing — we don't use Unix domain sockets or file handles for
`pi` communication at all, only plain file I/O (`open()`/`read()`) and OS
signals (`os.kill`/`killpg`). It fires on container/layer mounts, not on
hop activity, and does not correlate with the timing of this incident. No
action needed here.

### Why not switch away from file-tailing to a different pi transport
Investigated: `pi --mode rpc` exists as an option
(`pi --help` output, live in `crack-dev`) as an alternative to `--mode
json`. But the bug found here is a one-field misclassification in event
interpretation, not a flaw in the file-tailing transport itself — the
transport correctly delivered every byte pi wrote (589 well-formed JSON
events, no corruption, no data loss on the wire) and file-based output was
specifically chosen so `pi` survives a server reload (documented in
`_attempt_once`'s docstring). Replacing the transport would be a large,
risky rewrite in service of a bug that has a precise, well-understood,
one-place fix. Not recommended.

## Fix

**File: `.pi/crack/server/src/crack_server/pi_proc.py`**, `_process_stream_line` (~682-684):

```python
if etype == "agent_end" and event.get("willRetry"):
    # pi is continuing its own internal agent loop (auto-retry, multi-phase
    # orchestration, etc.) — not a process-exit signal. Keep tailing.
    return False
if etype in ("agent_end", "agent_settled"):
    sink.terminal = True
    return True
```

Backward compatible by construction: `event.get("willRetry")` is falsy for
any `agent_end` that omits the field (older pi versions, or the common
single-segment case), so simple hops behave exactly as before — terminal on
the first (and only) `agent_end`. Existing tests
(`test_persisted_then_clean_agent_end_still_returns_immediately`,
`test_auto_retry_end_after_progress_forces_resume` in
`tests/test_plan41.py`) are unaffected since `fake_pi.sh`'s scripted
`agent_end` events don't set `willRetry: true`.

No other file needs to change: turn persistence already happens at
`turn_end` ([pi_proc.py:656-665](.pi/crack/server/src/crack_server/pi_proc.py#L656-L665)),
independent of terminal detection, so turns produced during the
`willRetry: true` segments were already being parsed correctly by the
accumulator — they just weren't being read at all before this fix stops the
premature early return. The `EXIT_GRACE_SECONDS` detach/kill dance, the
`detached_pids` ledger/sweep (Fix B), and `_live_detached_manifest` reattach
logic are all still correct and still needed — they'll now simply fire once,
at genuine completion, as originally intended by the `-9` fix.

### Note (not part of this fix, flagging only)
With tailing now running for the task's true full duration, very large
multi-phase/sub-agent-orchestration prompts may legitimately approach the
per-hop `timeout_seconds` (1800s in this chat's manifest) where before they
were (incorrectly) truncated at ~20s. If long orchestration prompts like
this one become common, the timeout may need to be raised or made
per-request — a separate, follow-up tuning question, not addressed here.

## Verification

From `.pi/crack/server/`:

1. **New unit test** in `tests/test_plan41.py`, extending `fake_pi.sh` with
   a scripted behavior that emits an `agent_end` with `willRetry: true`
   followed by more turns, then a final `agent_end` (`willRetry: false`) +
   `agent_settled`. Assert: the tail does **not** stop at the first
   `agent_end`, all turns across both segments are persisted, and
   `sink.terminal` is only set True at the final event.
2. **Regression check**: `test_persisted_then_clean_agent_end_still_returns_immediately`
   and `test_auto_retry_end_after_progress_forces_resume` still pass
   unmodified.
3. Run: `uv run python -m pytest tests/` (bare `uv run pytest` fails on
   `cwd`/`sys.path` per existing project note).
4. **Live sanity** in `crack-dev`: re-run (or resume, if possible) a
   multi-phase/sub-agent chat similar to `1784586114784` through to genuine
   completion; confirm `docker exec crack-dev` shows the hop's manifest
   `offset` advancing past every `willRetry: true` `agent_end`, `chat.json`
   accumulating turns from every phase (not just the first), and `phase`
   only going `idle` once `agent_settled` (or a `willRetry: false`
   `agent_end`) truly arrives.
