# Bug report: reversed prewalk model-switch display in sub-agent trajectory

**Chat:** `1784885984252`  
**Sub-agent run:** `1784891376579_fcd043af`  
**Observed UI:** `⇄ prewalk plan complete — implementing on minimaxai/minimax-m3 (was nvidia/stepfun-ai/step-3.7-flash)`  
**Settings (`/settings`):** Plan · planner = `nvidia/minimaxai/minimax-m3`, Plan · implementer = `nvidia/stepfun-ai/step-3.7-flash`

## Verdict

**The runner is correct. The display code is wrong.**

Settings, spawn-time state, hop model selection, and the actual pi session all use the intended planner→implementer order. The reversed “implementing on M3 (was Step Flash)” line is a **false model-switch divider** produced when rendering the sub-agent card, not evidence that prewalk swapped in the wrong direction.

## What actually ran (ground truth)

From `run.json` for run `1784891376579_fcd043af`:

| Field | Value |
|---|---|
| `planner_model` | `nvidia/minimaxai/minimax-m3` |
| `implementer_model` | `nvidia/stepfun-ai/step-3.7-flash` |
| `plan` | `true` |

Turn stamping (abbreviated):

| Turn | Hop | Model stamped on turn | Tools | `reason` |
|---|---|---|---|---|
| 1 | 1 | `nvidia/minimaxai/minimax-m3` | `todo` | — |
| 2–16 | 1 | `nvidia/minimaxai/minimax-m3` | explore/read/bash | — |
| 17 | 1 | `nvidia/minimaxai/minimax-m3` | `edit` | **`swap`** |
| 19+ | 2 | `nvidia/stepfun-ai/step-3.7-flash` | implement | — |

`prewalk.swapped_already(turns)` → `True`  
`prewalk.model_for_phase(run, turns)` → `nvidia/stepfun-ai/step-3.7-flash` (implementer)

So: **M3 planned, first edit triggered swap, Step 3.7 Flash implemented.** This matches settings.

The pi session ndjson confirms the same models on assistant messages (`stepfun-ai/step-3.7-flash` on hop 2+).

## Root cause (display)

### 1. Annotations are appended after all turns instead of time-interleaved

In `chats._render_run_card`:

```python
spine = (
    list(turns)
    + _run_annotation_rows(chat_id, run_id)
    + list(state.get("traj_notes") or [])
)
transcript = "".join(render.render_turn_msgs(
    spine, errors=errors, include_text=True, model_state=render.new_model_state()
))
```

`_run_annotation_rows` docstring says annotations should “interleave into the `state['turns']` spine”, but the code **concatenates** them at the end. Chat-level rendering (`render_chat_msgs` → `trajectory_view.merge_exchange_sidecars`) **does** sort by `at`; sub-agent cards do not.

The run’s session has a `model_change` event at **session start** (2026-07-24T11:09:49), projecting to model `minimaxai/minimax-m3`. Because it is rendered **after** all 71 turns:

1. `model_state["model"]` is already `nvidia/stepfun-ai/step-3.7-flash` (last implementer turn).
2. The late `model_change` annotation sets `cur_model = minimaxai/minimax-m3`.
3. `render.render_turn_msgs` sees `prev != cur` and emits a divider.
4. `seen_todo` is still `True` from a later todo in the trajectory → divider is labelled **prewalk swap**.
5. Result: **“implementing on M3 (was Step Flash)”** — exactly backwards.

Reproducing this logic on the live run data yields two dividers:

| Index | Kind | prev | cur | prewalk_swap |
|---|---|---|---|---|
| 19 | turn | `nvidia/minimaxai/minimax-m3` | `nvidia/stepfun-ai/step-3.7-flash` | True (**correct**) |
| 72 | annotation | `nvidia/stepfun-ai/step-3.7-flash` | `minimaxai/minimax-m3` | True (**spurious, reversed**) |

The user-visible bogus line is index 72.

### 2. Secondary: model-id normalization mismatch

`trajectory_view.project_session_events` builds `model_change` model ids like this:

```python
model = str(event.get("modelId") or event.get("model") or "")
provider = str(event.get("provider") or "")
current_model = (
    f"{provider}/{model}"
    if provider and model and "/" not in model
    else (model or provider)
)
```

For pi events with `provider: "nvidia"` and `modelId: "minimaxai/minimax-m3"`, the `"/" not in model` check fails (modelId already contains `/`), so the projected id becomes **`minimaxai/minimax-m3`** instead of **`nvidia/minimaxai/minimax-m3`**.

Persisted turns use the full `nvidia/...` id from `persister.current_model = prewalk.model_for_phase(...)`. Even with correct chronological ordering, this prefix mismatch can produce **extra false dividers** between the session-start `model_change` badge and the first stamped turn.

## What is *not* broken

- `settings.py` / `/settings` — correct mapping of `plan_planner` / `plan_implementer`.
- `sub_agents/runner.spawn` — locks `planner_model` and `implementer_model` from settings at spawn time (verified in `run.json`).
- `prewalk.model_for_phase` — planner while planning, implementer after swap.
- `sub_agents/base.py` hop dispatch — sets `hop_model` from `prewalk.model_for_phase`, handles `reason == "swap"` by resuming on implementer.
- `pi_rpc.py` swap watch — fires on first `edit`/`write` after `todo`, persists the planner turn, returns `reason="swap"`.
- Card header badge (`_run_display_model`) — currently shows `nvidia/stepfun-ai/step-3.7-flash` for this run (correct next-hop model).

## How to fix

### Fix A (required): time-merge the sub-agent spine

In `_render_run_card` (or a small helper), merge `turns`, annotation rows, and `traj_notes` **by `at` epoch**, using the same keyed-sort pattern as `trajectory_view.merge_exchange_sidecars` / `render._merged_trajectory`:

- Primary rows (`turns`) keep list order on tie (monotonic `at`).
- Sidecar rows (annotations, notes, errors) sort by their `at`; on ties, land after spine rows.

This places the session-start `model_change` near the top (where it belongs) instead of after the entire trajectory.

### Fix B (recommended): normalize model ids before comparing

Add a small normalizer used by `render.render_turn_msgs` when tracking `model_state["model"]` and when handling `model_change` annotations, e.g.:

- If id lacks a provider prefix but a sibling field or known pattern supplies one, prepend it.
- Or: in `trajectory_view`, always emit fully-qualified ids for `model_change` (treat `modelId` as the tail even when it contains `/`: `f"{provider}/{model}"` when provider is set).

This prevents `minimaxai/minimax-m3` vs `nvidia/minimaxai/minimax-m3` from looking like a runtime switch.

### Fix C (optional hardening): don’t let session-start `model_change` drive swap dividers

`model_change` annotations are informational badges (“model → …”), not hop boundaries. Options:

- Only emit prewalk swap dividers from **turn** `model` field changes (ignore `model_change` annotations for divider logic), or
- Track whether a divider was already emitted for the todo→edit swap and suppress later annotation-driven swaps.

### Tests to add

1. Sub-agent card render: session with early `model_change` + later planner→implementer turn swap → exactly **one** prewalk divider, correct direction.
2. Model id normalization: `provider=nvidia`, `modelId=minimaxai/minimax-m3` projects to `nvidia/minimaxai/minimax-m3`.
3. Annotation interleaving: annotation at `t0` appears before turn at `t1` in rendered HTML order.

## Files involved

| File | Role |
|---|---|
| `src/crack_server/chats.py` | `_render_run_card` — builds unsorted spine (**primary bug**) |
| `src/crack_server/chats.py` | `_run_annotation_rows` — projects session annotations |
| `src/crack_server/render.py` | `render_turn_msgs` / `_model_switch_divider` — emits the misleading line |
| `src/crack_server/trajectory_view.py` | `model_change` projection — id normalization (**secondary bug**) |
| `src/crack_server/prewalk.py` | Model selection logic (**working as designed**) |
| `src/crack_server/sub_agents/base.py` | Hop runner + swap resume (**working as designed**) |

## Summary

The settings are honored at runtime. The scary reversed prewalk line is a **rendering artifact**: a session-start `model_change` annotation is processed **after** all implementer turns, so the UI thinks the run switched back from Step Flash to M3 for implementation. Fix the spine merge ordering (and normalize model ids) in the display path; no runner/prewalk logic change is required for this symptom.
