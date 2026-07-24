# Fix 2 — Stop the host-apply failure from re-enqueuing a new agent turn

**Segment 2 of 6. Implement after fix_1. Python-only, low risk.**

## What is broken

When a chat's extracted `patch.diff` fails to apply to the host, `finalize_chat_sandbox` appends a
**brand-new chat exchange** whose user prompt is the (up to 60 KB) `git apply` stderr dump, flips
the chat back to `chatting`, and enqueues another chat job. That new turn produces another patch,
which fails again, which enqueues again — **an infinite self-feeding loop**. On chat `1784739382358`
this produced 8 successive `source=patch_apply` exchanges and never stopped until the chat was
deleted.

Segment 1 removes the *cause* of most apply failures (the spurious `_data` deletions). This segment
removes the *engine*: a host-apply failure must never autonomously restart the agent.

## Design intent

A host `git apply` failure is an **environmental** problem (the patch cannot land on the host tree),
not something the chat agent can fix by "trying again." So on apply failure we should:
- record a durable, visible **error** on the chat (banner), and
- go **idle** and wait for a human,
- **never** append a new `patch_apply` exchange or enqueue a follow-up job.

(Two *other* re-prompt paths are legitimate agent feedback and are **out of scope** — leave them
alone: the oversized-file nag `enqueue_chat_patch_nag`, and the self-modification test-failure
`enqueue_chat_test_failure`. Those ask the agent to fix its *own* changes. Only the **host
apply-failure** path is the runaway loop.)

## Files & functions

All in `.pi/crack/server/src/crack_server/patch.py` unless noted.

- `finalize_chat_sandbox(chat_id, sandbox_name, *, forceful=False)` — around the
  `ok, err = await apply_patch_on_host(result.patch_path)` block. Today:
  ```python
  if not gated:
      ok, err = await apply_patch_on_host(result.patch_path)
      if not ok:
          enqueue_chat_apply_failure(chat_id, err, result.patch_path)   # <-- the loop
      elif self_mod:
          launch_health_watcher(chat_id, result.patch_path)
  ```
- `enqueue_chat_apply_failure(chat_id, stderr, patch_path)` — the helper that appends the exchange
  and enqueues a job (via `enqueue_chat_system_message`). It is called from **two** places:
  `finalize_chat_sandbox` (chat) and `notify_parent_apply_failure` (parent chat of a sub-agent).

## The fix

### 1. Add a non-enqueuing error recorder

Add a helper in `patch.py` that records a durable error banner on the chat **without** touching
`pending`/`exchanges` and **without** enqueuing:

```python
def record_chat_apply_failure(chat_id: str, stderr: str, patch_path: Path) -> None:
    """Surface a host `git apply` failure as a durable, visible error and go idle.
    Deliberately does NOT enqueue a new agent turn — a host apply failure is
    environmental and must not restart the chat (that was the patch-apply loop)."""
    resolved = str(patch_path.resolve())
    short = (stderr or "").strip()
    detail = short[-3000:]

    def _err(state: dict) -> dict:
        state["phase"] = "idle"
        state["error"] = "Your changes could not be applied to the host repo (git apply failed)."
        state["error_detail"] = f"{detail}\n\nThe full patch is at: {resolved}"
        return state

    paths.chat_state(chat_id).update(_err)
    logger.warning("patch: host apply failed for chat %s; recorded error, not re-enqueuing", chat_id)
```

### 2. Call it instead of the enqueue in `finalize_chat_sandbox`

```python
        if not ok:
            record_chat_apply_failure(chat_id, err, result.patch_path)   # was enqueue_chat_apply_failure
```

### 3. Same treatment for the sub-agent's parent-chat path

In `notify_parent_apply_failure`, the `parent_kind == "chat"` branch currently calls
`enqueue_chat_system_message(chat_id, message, source="patch_apply")`. Replace that call with
`record_chat_apply_failure(chat_id, stderr, patch_path)` (pass the raw `stderr`/`patch_path` through —
adjust the function signature/args as needed so it has them; it already receives `stderr` and
`patch_path`). Leave the `parent_kind == "run"` branch as-is for now (sub-agent internal; segment 3
handles stop durability for runs).

### 4. Remove the now-unused `enqueue_chat_apply_failure` (optional but preferred)

If nothing references `enqueue_chat_apply_failure` after steps 2–3, delete it (and
`format_apply_failure` if it is now unused — grep first). If unsure, leave them; do not leave a
half-wired call.

> Do **not** modify `enqueue_chat_patch_nag` or `enqueue_chat_test_failure`.

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
If `tests/test_patch.py` asserts the old apply-failure-enqueue behavior, update it to assert the new
behavior: on a failed host apply, the chat gets `error`/`error_detail` set, stays `idle`, and **no**
new `pending`/`exchanges` entry with `source == "patch_apply"` is added, and **no** job is enqueued.

### 2. Deterministic loop-cannot-start check (no LLM)
Force an apply failure and confirm it produces exactly one error and zero follow-up exchanges. The
simplest deterministic route is a unit test that calls `finalize_chat_sandbox` with a patch that
cannot apply (or mock `apply_patch_on_host` to return `(False, "boom")`) and asserts the chat state:
```
phase == "idle"; error is set; no exchange has source == "patch_apply";
the job queue got no new __chat__ job for this chat.
```
Grep `tests/` for how other tests fake the sandbox/queue and reuse that pattern.

### 3. End-to-end regression: the cascade cannot recur
Reproduce the original scenario (a chat whose patch would historically fail) and confirm it now
ends in a single visible error instead of looping. With fix_1 in place, apply failures are rare, so
to *force* the path you can temporarily set a chat's patch to something unapplyable, or rely on the
unit test in step 2. If you run a live chat:
```bash
CID=$(curl -s -o /dev/null -w '%{redirect_url}' -X POST http://localhost:9847/api/chats | sed 's#.*/chats/##')
curl -s -X POST "http://localhost:9847/api/chats/$CID/messages" \
  --data-urlencode 'msg=Create /workspace/hello_fix2.txt with the word hi. Then stop.' \
  --data-urlencode 'plan=' >/dev/null
# after it settles:
docker exec crack-dev bash -lc "jq -r '.exchanges[]?.source' /crack-harness-data/unscripted_chats/$CID/chat.json"
docker logs crack-dev 2>&1 | grep "$CID" | grep -c "podman exec -d"   # count pi spawns
```
**PASS:** the number of `patch_apply` exchanges is **0**, and the count of pi spawns is small and
bounded (it does not keep climbing over time). Clean up: `curl -s -X DELETE http://localhost:9847/api/chats/$CID`.

## Done when
A host apply failure records exactly **one** durable error banner, leaves the chat `idle`, adds
**no** `patch_apply` exchange, and enqueues **no** follow-up job — verified by a unit test and by a
live chat showing zero `patch_apply` exchanges.
