# Rolling Summarizer Compaction — After-Action Report

## What was implemented

Rolling Summarizer compaction (contract v1) for `crack_server`: when a chat or sub-agent session reaches **≥75%** of the model context window, the harness compacts **before the next hop** by summarizing older events and seeding a fresh pi session with the summary plus a retained tail.

### New: `src/crack_server/compaction.py`

Core module with:

- `COMPACTION_THRESHOLD = 0.75`, `RETAIN_TOKENS = 20_000`, `CHARS_PER_TOKEN = 4`
- `should_compact(sessions_dir, model)` — compares `session_usage` tokens to `models.context_window`
- `resolve_session_id(state, base_session_id)` — reads `pi_session_id` or falls back to base
- `_estimate_event_tokens`, `_message_count`, `_find_cutoff_index` — tool-group-safe tail retention
- `_events_transcript` — plain-text prefix for summarization
- `SUMMARY_PROMPT` — 5-heading Codex/OpenCode-style structure
- `generate_summary(transcript, model)` — `arun_pi_text` with deterministic `_fallback_summary` on failure
- `seed_compacted_session` — writes new immutable `.jsonl` (session event + summary user message + tail)
- `compact_if_needed(...)` — orchestrates kill → summarize → seed → state update → traj note; errors are non-fatal

### Modified integration points

| File | Change |
|------|--------|
| `context_guard.py` | Added `needs_compaction()` delegating to `compaction.should_compact`; kept force-stop helpers for tests |
| `chat_engine.py` | Calls `compact_if_needed` before each hop; added optional `base_session_id` param |
| `chats.py` | Passes `base_session_id=f"unscripted-{chat_id}"` and resolves active session id |
| `sub_agents/base.py` | Calls `compact_if_needed` before `arun_agent_hop` with `subagent-{run_id}` base |
| `render.py` | Compaction notes show token/message/duration stats inline |
| `static/app.css` | Orange `.traj-note--compaction` styling |

### New tests: `tests/test_compaction.py`

Covers threshold, cutoff/tool-group safety, fallback summary, seed jsonl, state/note updates, render HTML. Uses monkeypatch for `session_usage` and `generate_summary`.

## Design choices

1. **Immutable session handoff** — Old `.jsonl` files are left untouched; a new timestamped file is written and `pi_session_id` advances to `{base}-c{N}`.
2. **Tool-group safety** — Cutoff alignment never starts the retained tail with a lone `toolResult`; assistant+toolResult groups move together.
3. **Estimate vs meter mismatch** — When char-based estimates under-count but the meter is full, fall back to summarizing the oldest ~75% of events so compaction still runs.
4. **Non-fatal failures** — Compaction errors append an `err` traj note and return the prior session id so hops continue.
5. **Cache invalidation** — Clears `trajectory_view` cache and drops superseded `context_stats` usage cache entry after seeding.

## How to build/test

```bash
cd /workspace/.pi/crack/server
poetry run pytest tests/test_compaction.py tests/test_context_guard.py -q
```

Result: **23 passed**.

## Follow-ups

- Add `needs_compaction` to context meter UI (optional pre-hop indicator).
- Tune proportional fallback split (75%) or tie retention to reported usage ratio when estimates diverge.
- Consider compacting across multiple session files if pi ever shards one logical session.
- Wire `context_guard` import cleanup in `chats.py` (currently unused).
