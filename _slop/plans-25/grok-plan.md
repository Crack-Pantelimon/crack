# Plan: Stop does not halt; patch-apply loop; trajectory order broken

Chat under investigation: http://localhost:9847/chats/1784739382358  
Date: 2026-07-22  
Method: browser (cursor-ide-browser) + `docker exec` / `docker logs` on `crack-dev` + code read of stop/retry/patch/trajectory paths.

---

## Verdict

Three distinct bugs stacked on the same chat:

1. **STOP does not stick** — the hop can classify as `stopped`, but the worker then **finalizes the sandbox**, hits a failing `git apply`, and **re-enqueues a new agent turn** that clears `stop_requested`.
2. **Global no-progress / crash retry “giving up” also does not stick** — same finalize → patch-fail → re-enqueue loop, so the chat keeps cycling until deleted.
3. **On refresh, durable errors are dumped as a block at the bottom** — `trajectory_view.merge_exchange_sidecars` appends all exchange errors after the whole session projection instead of interleaving by `at`.

User intent for STOP: kill + suspend until the human sends another message. Sub-agent STOP should do the same for that run only (not the parent). Current code fails that for chats because post-stop/post-error sandbox finalize can autonomously restart work.

---

## Live evidence (chat `1784739382358`)

### Browser

- Title: screenshot task; sidebar shows **Stop all**; phase reads idle while fatal banner says  
  `pi crashed mid-turn after 7 attempts (no progress in 7 consecutive attempts)`.
- Trajectory DOM order (refresh / full render):
  - Top: session annotations + many **user prompt** rows (original + dozens of **Patch application failed.**).
  - Bottom: one contiguous block of **⚠ pi crashed mid-turn (attempt N · …)** rows, then the fatal banner.
  - Errors are **not** interleaved with the prompts they belong to (timestamps say 4m…0s ago while prompts are clustered above).

### `chat.json` (via `docker exec crack-dev`)

| Field | Value |
|-------|--------|
| `phase` | `idle` |
| `stop_requested` | `True` (after repeated STOP clicks; stuck flag while idle) |
| `error` | `pi crashed mid-turn after 7 attempts (no progress in 7 consecutive attempts)` |
| exchanges | 9: **1× human**, then **8× `source=patch_apply`** |
| per-exchange errors | 29 total crash rows across exchanges |

Exchange timeline (abbrev.):

- EX0 `human` → 6 crash errors  
- EX1–EX8 `patch_apply` (“Patch application failed…”) → more crashes / streak exhaustion  

So the “engine keeps retrying by itself” is not only in-hop `HARD_RETRY_DELAYS` — it is **new exchanges** created by the patch-apply failure path.

### `docker logs crack-dev` (smoking gun sequence)

Repeated pattern:

```text
unscripted-chat hop 1: attempt N finished reason=stopped ...
unscripted-chat: exchange K done for 1784739382358 (reason=stopped)
WARNING:  patch apply ['--3way'] failed ...
WARNING:  patch apply ['--reject'] failed ...
unscripted-chat hop 1: full prompt:
Patch application failed.
+ podman exec -d crack-sbx-1784739382358 pi ... 'Patch application failed.
```

Also seen without STOP: crash → retry streak → give up → same patch-apply re-prompt.

STOP HTTP handler itself works at the kill layer:

```text
chats: stop requested for 1784739382358 (killed=True)
POST /api/chats/1784739382358/stop → 200
```

Sandbox container `crack-sbx-1784739382358` was observed cycling (create / sleep infinity) while this loop ran.

---

## Root causes

### A. Patch finalize restarts the chat after stop / after give-up

`patch.enqueue_chat_system_message` / `enqueue_chat_apply_failure`:

- appends a pending message (`source=patch_apply`)
- sets `phase = "chatting"`
- **forces `stop_requested = False`**
- `queue.enqueue_exclusive(..., CHAT_JOB_SLUG, ...)`

`finalize_chat_sandbox` always tries host apply when the sandbox has a patch; on failure it calls `enqueue_chat_apply_failure`.

Called from `chats.run_chat`:

- after a normal idle drain
- **and** on the stop path: `finalize_chat_sandbox(..., forceful=True)`

So STOP → kill pi → (maybe) reason=stopped → **still extract/apply patch** → apply fails on bad `_data/...` blob → **autonomous new turn**.

That matches “STOP should suspend until user sends another message” and “retry limit catches it but then keeps cycling.”

### B. `stop_requested` is cleared too early, so the worker stop-halt branch is skipped

`chat_engine.run_exchange` `_finish`:

```python
s["phase"] = stopped_phase if reason == "stopped" else "idle"
s["stop_requested"] = False   # cleared on every successful exchange end
```

`chats.run_chat` only takes the hard stop exit **after** `run_exchange`:

```python
if stop_check():  # reads stop_requested — already False
    await patch.finalize_chat_sandbox(..., forceful=True)
    # halt + return
```

So after a clean `reason=stopped`, `stop_check()` is often **false**, the dedicated halt branch is skipped, and the worker falls through into the idle finalize path — which is exactly where patch-apply re-enqueue happens.

`record_chat_errors` also clears `stop_requested` on exception paths.

### C. In-hop crash retry does not poll STOP between attempts

`_run_hop_with_retries` (`pi_proc.py`):

- on crash: record error → sleep `HARD_RETRY_DELAYS` → spawn again
- `stop_check` is only consulted **after** an attempt ends (inside `_attempt_once` / reattach)
- no `stop_check` before/during `_async_hard_backoff_sleep`

So if STOP lands during backoff, the next attempt can still start unless something else kills it. Once the attempt ends with `stop_requested`, classification as `stopped` works (logs show `reason=stopped`). The bigger leak is A+B after that.

`HARD_RETRY_DELAYS = [1,3,6,9,16,27]` → streak cap = 7 attempts → matches the fatal banner text.

### D. Trajectory ordering (refresh)

`chats.render_chat_msgs` now projects from session ndjson via `trajectory_view`, then:

```python
rows = trajectory_view.merge_exchange_sidecars(projected, exchanges)
```

In `merge_exchange_sidecars`:

1. Projected session rows (prompts/turns/annotations) first.
2. Missing exchange prompts inserted near the front (`out.insert(len(qa_rows), ...)`).
3. **All** exchange `errors[]` appended with `out.extend(error_rows)` — comment even says “Errors after the trajectory”.

Contrast: older `render._merged_trajectory` **does** time-merge turns+errors by `at`. That merge is bypassed for the session-projection path.

Result on this chat: prompts (incl. every patch_apply prompt) at top; all 29 crash rows chunked at bottom — matches the browser.

Extra noise: multiple `sessions/*.jsonl` files → duplicated session/model annotations and repeated “go to google.com…” prompts in the projection.

---

## Sub-agent STOP (expected vs current)

Expected: stop one run hard; parent chat continues.

Current `SubAgentPersona.request_stop`:

- sets `stop_requested`, phase `stopped`, kills run pid
- `cascade=False` (UI stop): `runner.finish(run_id, "stopped")`
- `cascade=True` (chat Stop all): skips parent resume; `extract_run_patch(..., mark_pending=False)` so stopped children should not push patches

Per-run stop is closer to correct than chat stop. Residual risks:

- sub-agent retry/nudge loops should honour `stop_requested` the same way (same `_run_hop_with_retries` gap during backoff)
- patch-nag paths (`enqueue_subagent_patch_nag`) can still re-queue a stopped run’s step if finalize/nag races — worth hardening with the same “do not enqueue if stopped / stop_requested” rule

---

## Fix plan (recommended)

### 1. Make STOP a durable suspend (chat + sub-agent)

- On STOP: set `stop_requested=True`, kill pi (host pid **and** sandbox session), clear `pending`, set phase idle/stopped, **do not clear `stop_requested` until the next human message** (or explicit retry).
- `chat_engine._finish`: if `reason == "stopped"`, **keep** `stop_requested` (or set a `suspended_by_stop` flag).
- `enqueue_chat_system_message` / patch nag / apply-failure / child_inbox: **refuse to enqueue** (or queue but do not auto-run) when `stop_requested` or phase is stop-suspended.
- `run_chat` after any exchange: if stop suspended, **skip** `finalize_chat_sandbox` apply-failure requeue (still OK to destroy sandbox / extract patch to disk without waking the agent). Prefer: extract+destroy only; never `enqueue_chat_apply_failure` after user STOP.
- Same policy for sub-agent `request_stop`: no patch-nag / step enqueue until human Retry or parent intentionally resumes.

### 2. Honour STOP inside the crash-retry loop

In `_run_hop_with_retries`, before sleep and before spawning the next attempt:

```text
if p.stop_check and p.stop_check(): return "stopped"
```

Interruptible sleep (chunked sleep that re-checks stop) is nicer but optional if kill+flag are solid.

### 3. Break the poison patch-apply loop even without STOP

After streak exhaustion / over-budget:

- surface the error and go idle
- **do not** auto-enqueue `patch_apply` as a new user turn that restarts the agent forever
- options: one-shot notify without enqueue; or enqueue only if apply failure is new and chat is not already in error/idle-after-budget; or require human Send to continue

The failing patch on this chat (`_data/3d_data_v2/...bytes does not match index`) is the fuel; the enqueue is the engine.

### 4. Fix trajectory time order on refresh

In `trajectory_view.merge_exchange_sidecars` (or at render):

- merge error rows into the projected timeline by `at` (reuse / share `_merged_trajectory` semantics)
- do not `extend` all errors at the end
- insert missing prompts by timestamp (or after their exchange’s neighbors), not always at index 0
- consider projecting a **single** session file / deduping multi-session restarts for one chat id

### 5. Tests to add

- STOP mid-crash-retry → no further attempts; no new `patch_apply` exchange; `stop_requested` remains until human message.
- Streak give-up with failing `finalize_chat_sandbox` → idle, **one** error banner, zero autonomous follow-up jobs.
- Sub-agent STOP → run phase stopped; parent chat job not stopped; no child patch_pending wake.
- `merge_exchange_sidecars` with interleaved prompt/turn/error `at` values → render order matches timestamps.

---

## Files to touch

| Area | Files |
|------|--------|
| Stop / worker | `chats.py`, `chat_engine.py`, `steprun.py` (`record_chat_errors`) |
| Retry loop | `pi_proc.py` (`_run_hop_with_retries`) |
| Patch requeue | `patch.py` (`enqueue_chat_system_message`, `finalize_chat_sandbox`) |
| Sub-agent stop | `sub_agents/base.py`, maybe `sub_agents/runner.py` |
| Trajectory order | `trajectory_view.py` (and/or `chats.render_chat_msgs`) |
| Tests | `tests/test_crash_retry.py`, new stop+patch / trajectory order tests |

---

## Out of scope / separate issues seen

- Underlying pi crash mid-turn in sandbox (why attempts die in ~1s) — real, but STOP/order bugs fire even when stop classification works.
- Host `git apply` conflict on binary `_data/...` cache files — why finalize keeps failing; fix loop first, then optionally exclude those paths from sandbox patches.
- Sidebar **Stop all** visible while idle (UX); not the root cause.
