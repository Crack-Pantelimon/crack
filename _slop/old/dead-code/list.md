# crack-server — dead code, half-implemented features & cleanup report

Scope: `/.pi/crack/server/src/crack_server/` (+ its `tests/`).
Date: 2026-07-23. Investigated via `docker exec crack-dev` against the live
harness data in `/crack-harness-data`.

This file covers two **bugs that were fixed** in this pass (image re-hosting
regression; stuck "running" tool dots) plus a broader audit of dead code,
orphaned functions, stale docstrings, and tests that exercise dead code.

---

## PART 1 — The two reported bugs (fixed)

### Bug A — image re-hosting regressed (images vanish once the sandbox is gone)

**Symptom.** Images the agent reads / analyzes are copied out of the sandbox
and re-hosted so thumbnails keep rendering after the sandbox disappears. This
stopped working — thumbnails only survive while the source file still happens
to exist in the server's own filesystem.

**Root cause — a regression + an architectural gap:**

1. **Regression in `af0d49f` ("fix chat 6").** `render_chat_msgs`
   ([chats.py](../../.pi/crack/server/src/crack_server/chats.py)) was switched
   from rendering the persisted `exchanges[].turns` (whose `tool_blocks` carry a
   durable `media:[{src,url}]` from `TurnPersister`) to **re-projecting the pi
   session ndjson** (`trajectory_view.project_sessions_dir`) and re-deriving
   media at render time via `attach_media_to_blocks`. The old
   persisted-media render path (`render.render_exchanges`, see §2.1) was
   orphaned in the same commit.

2. **`attach_media_to_blocks` needs the source bytes to reproduce the URL.**
   Saved copies are named `sha1(bytes)[:12]+ext`. At render time it re-reads the
   *source path* to recompute that name; when the source is gone it returns
   `None` and drops the media — even though the durable copy still sits in the
   chat's `media/` dir. So "survives sandbox deletion" was defeated.

3. **Sandbox filesystem boundary.** Sandboxed chats keep generated images (e.g.
   `/workspace/google_screenshot.png` screenshots) in the **overlay upper**
   (`/crack-harness-data/overlays/<conv_id>/upper/…`), invisible to the server's
   own `/workspace`. So persist-time capture (which runs on the host FS) saved
   **nothing** for sandbox images — verified: chat `1784749870691` has the saved
   copy `media/f30c68526b34.png` but **0** `media` fields in `chat.json`. The
   only reason its thumbnail still renders is that the file *coincidentally*
   leaked into the server's real `/workspace` later.

**Fix applied:**
- `steprun.attach_media_to_blocks(…, conv_id=None)` now:
  - resolves sandbox `/workspace/*` sources from the **overlay upper** while the
    run is live (`sandbox.overlay_upper_dir` — new helper);
  - writes a durable `media/index.json` manifest mapping `source path → saved
    filename`, and on a later render (source gone) serves the remembered copy
    from that manifest. This makes rendering independent of the source.
- Threaded `conv_id` through `project_sessions_dir` (chat render, `conv_id=chat_id`)
  and through `TurnPersister` → both persister sites (`chat_engine` `conv_id=ident`,
  `sub_agents/base` `conv_id=run_id`), so both the chat-render path and the
  sub-agent-run persist path capture sandbox images correctly.

Verified with a direct harness script: media attaches with source present, a
manifest is written, media **survives source deletion**, and `/workspace` sources
resolve from the overlay upper. Files: `steprun.py`, `sandbox.py`,
`trajectory_view.py`, `chats.py`, `chat_engine.py`, `sub_agents/base.py`.

### Bug B — tool calls stuck "running" (blue dot) in the live trajectory

**Symptom.** While watching a live trajectory, a tool call keeps its blue
"pending" dot even after it returned and the agent moved on; a full page refresh
shows the real result.

**Root cause.** The live watcher does an **append-only delta**:
`fetchChatDelta` ([static/app.js](../../.pi/crack/server/src/crack_server/static/app.js))
computes `after = lastChatMsgIndex()` and requests `/status?after=N`;
`wrap_chat_content` ([chats.py](../../.pi/crack/server/src/crack_server/chats.py))
returns only messages with index `> after`, swapped `beforeend`. A turn that was
first rendered while its tool result had not yet landed in the session ndjson is
frozen at its index — the delta never revisits it, so its `_tool_dot_class`
"pending" dot ([render.py](../../.pi/crack/server/src/crack_server/render.py))
never settles until a full reload re-projects it.

**Fix applied.** `wrap_chat_content` now also re-emits the **boundary message**
(index `== after`, the last one the client already has) as an `hx-swap-oob="true"`
copy, so the in-flight turn is replaced in place on each poll and its dots settle;
strictly-new messages (`i > after`) are still appended as before. `_tag_chat_msg`
grew an `oob` flag. Files: `chats.py`.

---

## PART 2 — Dead / orphaned code

### 2.1 `render.render_exchanges` — dead in prod, still tested (masks Bug A) ⚠️
[render.py:656](../../.pi/crack/server/src/crack_server/render.py#L656)

The **old** chat-exchange renderer ("plan 4.3 A3"): walks `exchanges`, renders a
user bubble + `exchange.get("media")` prompt thumbs + each turn's `tool_blocks`
media. It has **zero production callers** (orphaned by `af0d49f`) — its only
reference is `tests/test_vision_media.py::test_render_exchanges_shows_prompt_thumbs_from_exchange_media`.
That test passes and looks like it proves exchange-media rendering works, while
the real path (`project_sessions_dir`) doesn't use it — i.e. **a green test over
dead code that masked exactly the Bug A regression**. Recommend: delete the
function and re-point/remove the test, or (better) migrate its prompt-thumb logic
into the live path if prompt-attachment thumbs are wanted.

### 2.2 Dead sync-diff cluster in `patch.py`
Leftovers from when patch production ran synchronously in the (now retired)
out-of-process worker. All have **0 references** in `src` and `tests`:

- `ensure_baseline_sync` [patch.py:188](../../.pi/crack/server/src/crack_server/patch.py#L188)
- `capture_baseline_sync` (only caller is `ensure_baseline_sync`)
- `_produce_diff_sync` [patch.py:253](../../.pi/crack/server/src/crack_server/patch.py#L253)
- `_write_tree_sync` (only caller is `_produce_diff_sync`)
- `_stage_for_patch_sync` (only caller is `_produce_diff_sync`)
- `apply_patch_to_host` [patch.py:392](../../.pi/crack/server/src/crack_server/patch.py#L392) (sync twin; async `apply_patch_on_host` is the live one)

The rest of the `_sync` layer (`_git_in_sandbox_sync`, `apply_patch_to_sandbox_sync`,
`extract_patch_sync`) **is** live — don't remove those. Recommend deleting the six
above as a unit.

### 2.3 `chat_engine.run_exchange_sync` — orphaned by the stages removal
[chat_engine.py:48](../../.pi/crack/server/src/crack_server/chat_engine.py#L48)

Sync `asyncio.run(...)` wrapper whose docstring says it exists "for the Finished
stage's review chat, dispatched via `asyncio.to_thread`". The fixed stages were
removed (`a809eaf` "remove fixed stages, keep only sub agents"); **0 callers**.
Dead.

### 2.4 `chats._clear_ui_prep` — never called (also a latent leak)
[chats.py](../../.pi/crack/server/src/crack_server/chats.py) (`def _clear_ui_prep`)

Meant to "drop previous prep timings so a new exchange starts a fresh debug
strip", but nothing calls it. Consequence: `state["ui_prep"]` is only ever
appended to (`_append_ui_prep`), so the sandbox/first-byte prep rows **accumulate
across every exchange** in a chat and render forever. Either wire it in at
exchange start or delete it and stop appending. (Note: Bug A's plan item 6 also
touches these prep rows — reconcile there.)

### 2.5 `render.render_turns_trajectory` — unused wrapper
[render.py:651](../../.pi/crack/server/src/crack_server/render.py#L651)
Thin `"".join(render_turn_msgs(...))` helper, **0 callers**. Delete.

### 2.6 `paths.worker_lock_path` — unused
[paths.py:84](../../.pi/crack/server/src/crack_server/paths.py#L84)
Returns `harness_dir()/worker.lock`; **0 callers**. The worker moved back
in-process (asyncio lifespan), so the on-disk worker lock is gone. Delete.

### 2.7 `worker.main` — retired tombstone entrypoint
[worker.py](../../.pi/crack/server/src/crack_server/worker.py) (`def main`)
Raises `SystemExit("crack-worker is retired …")`. There is **no `crack-worker`
script** in `pyproject.toml` (`[project.scripts]` only defines `crack-server`), so
`main()` and the `if __name__ == "__main__"` block are unreachable. Harmless
tombstone; can be deleted for tidiness.

---

## PART 3 — Stale docs / redundant work (not strictly dead, worth fixing)

### 3.1 Stale docstring: `pi_rpc.py` still advertises removed json-mode
[pi_rpc.py:4](../../.pi/crack/server/src/crack_server/pi_rpc.py#L4)
Module docstring: "Set `CRACK_PI_JSON=1` to force the legacy json-mode path
during transition." But `pi_proc.py` now treats `CRACK_PI_JSON=1` as a **hard
error** ("no longer supported — agent hops use RPC mode",
[pi_proc.py:417](../../.pi/crack/server/src/crack_server/pi_proc.py#L417)). The
docstring is misleading — update it.

### 3.2 Redundant persist-time media capture for chats
After Bug A's fix, chat rendering resolves media at render time
(`project_sessions_dir` with `conv_id`). The persist-time media attached into
`exchanges[].turns[].tool_blocks` is now only read by the dead `render_exchanges`
(§2.1) — i.e. a **wasted write for chats**. It is still required for **sub-agent
runs** (`run_page` renders `state["turns"]` via `render_turn_msgs`, which does
render `block["media"]`), so the code can't simply be dropped — but once
`render_exchanges` is deleted, the chat-exchange `media` field is pure overhead.
Consider gating persist-time capture to the run path only.

### 3.3 Test-only cache-reset helpers
`trajectory_view.clear_cache` [trajectory_view.py:403] and
`registry.clear_cache` [registry.py:78] are referenced only from tests. Legitimate
test scaffolding, but flag them so they aren't mistaken for prod API.

---

## PART 4 — Suggested cleanup order (low risk → higher)

1. Delete §2.5 `render_turns_trajectory`, §2.6 `worker_lock_path`, §2.7
   `worker.main` (pure deletions, 0 callers).
2. Delete §2.3 `run_exchange_sync` and §2.2 patch sync-diff cluster (0 callers,
   verify with `grep -rnw` first).
3. Resolve §2.1 `render_exchanges` + its test together — this is the one that
   actively misleads. Decide: migrate prompt-thumb rendering into the live path,
   or delete both.
4. Fix §2.4 `_clear_ui_prep` (wire in or delete + stop appending) — it's a real
   accumulating-state leak, not just dead code.
5. Fix §3.1 stale docstring; revisit §3.2 once §2.1 is resolved.

## Verification notes
- Full server test suite: **196 passed** with the Bug A/B fixes in place
  (`python -m pytest` from `.pi/crack/server`, in-container venv). One unrelated
  flaky test (`test_async_worker.py::test_worker_caps_concurrent_inflight`,
  timing-based) failed once under load and passed in isolation.
- Dead-symbol detection: AST enumeration of top-level defs cross-referenced with
  literal grep over `src/` + `tests/` (route handlers excluded — they register via
  decorators, not name references).
