# Fix 3 — Make STOP durable (suspend until the next human message)

**Segment 3 of 6. Implement after fix_2. Python-only, medium risk (touches shared worker state).**

## What is broken

Clicking **STOP** sets `stop_requested = True` and kills the running pi. But `stop_requested` is
cleared back to `False` in ~5 automatic places (every exchange end, every internal enqueue), so
within ~1 second the chat clears its own STOP and resumes. The user has to click STOP many times and
it still restarts. Intended behavior: **STOP halts the chat and keeps it suspended until the human
sends another message** (or explicitly retries). Stopping a sub-agent should halt that run only,
never the parent.

## Where `stop_requested` is (wrongly) cleared

There are two **legitimate** places that clear it (a human explicitly resuming) — **keep these**:
- `chats.py` `post_message._begin` (~line 1061): a new human message. **Keep the clear.**
- `chats.py` `answer_chat_question` (~line 1097): a human answering an ask_user question. **Keep.**

There are the **automatic** clears that must be **removed** (they are what breaks STOP):

1. `chat_engine.py` — `run_exchange._finish` (~line 129): clears on **every** exchange end.
2. `patch.py` — `enqueue_chat_system_message._enqueue` (~line 466): clears on every internal enqueue.
3. `patch.py` — `finalize_chat_sandbox` `_bump` (the `needs_nag` branch, ~line 633): clears on nag.
4. `chats.py` — `_merge_child_inbox._enqueue` (~line 1215): clears when a child report arrives.
5. `steprun.py` — `record_chat_errors._fail` (~line 276): clears at the end of the error path.

`stop_chat._flag_stop` (chats.py ~1127) sets it `True` — keep. `_pop_pending` (chats.py ~1238)
already drains `pending` and returns `None` while `stop_requested` is set (so the worker idles) —
keep; it must **not** clear the flag.

## The fix

### 1. Remove the five automatic clears

- `chat_engine.py`, `_finish`:
  ```python
  def _finish(s: dict) -> dict:
      s["phase"] = stopped_phase if reason == "stopped" else "idle"
      # (delete this line) s["stop_requested"] = False
      if reason == "empty":
          s["error"] = "model returned empty responses"
          s["error_detail"] = ""
      return s
  ```
- `patch.py` `enqueue_chat_system_message._enqueue`: delete the `state["stop_requested"] = False` line.
- `patch.py` `finalize_chat_sandbox` `_bump`: delete the `s["stop_requested"] = False` line.
- `chats.py` `_merge_child_inbox._enqueue`: delete the `state["stop_requested"] = False` line.
- `steprun.py` `record_chat_errors._fail`: delete the trailing `s["stop_requested"] = False` line.
  Keep the existing `if not s.get("stop_requested"):` guard that suppresses the error text for a
  stop-triggered exception (a STOP must not be dressed up as an error).

### 2. Guarantee the human-resume paths clear it

`post_message._begin` and `answer_chat_question` already set `stop_requested = False` when a human
message/answer arrives — confirm those clears remain. Additionally, find the **retry-from-error**
path (grep for `grant_error_budget` and its callers across `crack_server/`, e.g. a
`retry_from_error` handler / route). Wherever a human explicitly retries a chat, ensure it also sets
`stop_requested = False` (add it if missing) — otherwise a retry after a stop would no-op.

### 3. Sanity of the worker halt path (no change expected, just verify by reading)

With the clears gone: after STOP, `run_chat`'s loop calls `_pop_pending`, which — because
`stop_requested` is `True` — drains `pending` and returns `None`, so the worker goes `idle` and
returns without spawning pi. Any stray job enqueued by an internal path (nag/system message) that
still fires will hit the same guard and harmlessly no-op. Confirm `run_chat` (chats.py ~1274) and
`_pop_pending` (~1232) behave this way after your edits; do not add new enqueues.

### 4. Sub-agent parity (lightweight)

Confirm the sub-agent stop path stops only its own run and does not clear the **parent chat's**
`stop_requested`. Look at `sub_agents/base.py` `request_stop` and `sub_agents/runner.py`. If a
child-stop path writes the parent chat's `stop_requested = False`, remove that. (If sub-agent code
is large, keep this to a read + targeted removal; the parent-chat durability above is the priority.)

## Build / restart
```bash
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh
```

## Verify

### 1. Unit tests green
```bash
docker exec crack-dev bash -lc \
  'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
```
Add/adjust a test (see `tests/` for existing chat-state tests) asserting: after `stop_chat`,
running an exchange finish (`_finish`) and an internal enqueue (`enqueue_chat_system_message`) both
**leave `stop_requested` True**; and `post_message` **clears** it.

### 2. Live: STOP sticks and suspends
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
# a longer task so there is time to stop it mid-run
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=List every file under /workspace/.pi recursively, then summarize what each top-level dir does in detail.' \
  --data-urlencode 'plan=' >/dev/null
sleep 8
curl -s -X POST "http://localhost:9847/api/chats/$CID/stop" >/dev/null
# watch for ~30s: stop_requested must stay True, phase must stay idle, and NO new pi spawns
for i in $(seq 1 10); do
  docker exec crack-dev bash -lc "jq -r '\"stop=\"+(.stop_requested|tostring)+\" phase=\"+.phase' /crack-harness-data/unscripted_chats/$CID/chat.json"
  sleep 3
done
echo "--- pi spawns AFTER stop (should be 0 growth) ---"
docker logs --since 40s crack-dev 2>&1 | grep "$CID" | grep -c "podman exec -d"
```
**PASS:** `stop_requested` stays `true`, `phase` stays `idle`, and no new `podman exec -d` spawns
appear after the stop. The count in the last command should stop growing.

### 3. Live: a human message resumes it
```bash
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Just reply the word RESUMED and stop.' >/dev/null
sleep 6
docker exec crack-dev bash -lc "jq -r '\"stop=\"+(.stop_requested|tostring)+\" phase=\"+.phase' /crack-harness-data/unscripted_chats/$CID/chat.json"
```
**PASS:** after the human message, `stop_requested` is `false` and the chat runs again (phase
`chatting` then `idle`). Clean up: `curl -s -X DELETE http://localhost:9847/api/chats/$CID`.

## Done when
STOP leaves `stop_requested` `True` and the chat idle with no further pi spawns until a human
message arrives, at which point it clears and resumes — verified live and by unit tests.
