# Fix 4 — Interleave error rows into the trajectory by timestamp

**Segment 4 of 6. Implement after fix_3. Python-only, low risk (render path only, no worker state).**

## What is broken

On refresh, a chat's trajectory shows all the prompts/turns at the top and then **every error row
dumped in one block at the bottom**, out of time order — even though each error happened between
specific turns. The errors have real timestamps but are appended after the whole trajectory instead
of interleaved.

## Root cause

`.pi/crack/server/src/crack_server/trajectory_view.py`, function `merge_exchange_sidecars` ends with:

```python
    # Errors after the trajectory (attempt metadata is relative to hops).
    out.extend(error_rows)
    return out
```

`out.extend(error_rows)` places all errors at the end. The rest of the function builds `out` in the
correct chronological order from the projected session stream (`kind` rows carry a `timestamp` ISO
string) plus harness prompt rows; the error rows carry an `at` epoch float. They just need to be
**merged by time** instead of appended.

There is already a reference implementation of exactly this merge for the *other* (non-session)
render path: `render._merged_trajectory(turns, errors)` in `.pi/crack/server/src/crack_server/render.py`
— it sorts by `at`, carries forward missing timestamps, and breaks ties so an error sorts *after*
the turn it follows. Reuse that shape here, adapted to the session rows' `timestamp` field.

## The fix

In `trajectory_view.py`, add a small epoch helper and replace the final `out.extend(error_rows)`
with a time-ordered merge.

### 1. Add a row-epoch helper (module level in `trajectory_view.py`)

```python
from datetime import datetime, timezone

def _row_epoch(row: dict) -> float | None:
    """Best-effort epoch for a trajectory row: prefer a harness `at` float, else
    parse a session event's ISO `timestamp` (…Z). None when neither is present."""
    at = row.get("at")
    if at is not None:
        try:
            return float(at)
        except (TypeError, ValueError):
            pass
    ts = row.get("timestamp")
    if ts:
        try:
            return datetime.fromisoformat(str(ts).replace("Z", "+00:00")).timestamp()
        except ValueError:
            pass
    return None
```

### 2. Preserve the timestamp when converting a `session_user` row to a `user_prompt` row

In the loop that rewrites `session_user` rows into `user_prompt` rows, carry the source row's
`timestamp` onto the new dict so it keeps its time position, e.g.:

```python
            out.append({
                "kind": "user_prompt",
                "id": row.get("id") or f"prompt-{len(out)}",
                "timestamp": row.get("timestamp"),   # <-- add this
                **meta,
            })
```
(Do the same `"timestamp"` passthrough for any other place that rebuilds a projected row into a new
dict, so no row loses its time key. `meta` must not overwrite it — put `**meta` before the explicit
`timestamp` if `meta` could contain one, or just ensure `meta` has no `timestamp`.)

### 3. Replace the final append with a time merge

Replace:
```python
    # Errors after the trajectory (attempt metadata is relative to hops).
    out.extend(error_rows)
    return out
```
with a merge that keeps `out` in order and slots each error by its epoch (mirrors
`render._merged_trajectory`):

```python
    # Merge error rows into the projected stream by time instead of dumping them
    # at the end. `out` rows carry a monotonic (carry-forward) epoch; errors sort
    # by their own `at`, and on ties land after the row they follow.
    keyed: list[tuple[float, int, int, dict]] = []
    last = 0.0
    for idx, row in enumerate(out):
        ep = _row_epoch(row)
        if ep is None or ep < last:
            ep = last            # carry forward so out order is preserved
        else:
            last = ep
        keyed.append((ep, 0, idx, row))
    for idx, err in enumerate(error_rows):
        ep = _row_epoch(err)
        keyed.append((ep if ep is not None else last, 1, idx, err))
    keyed.sort(key=lambda item: (item[0], item[1], item[2]))
    return [payload for _, _, _, payload in keyed]
```

Leave the `qa_rows` (ask_user Q&A cards) where they are — they legitimately front the exchange they
answered and typically have no timestamp; the carry-forward keeps them ordered.

> Note (context, not a task here): the *duplicate* "go to google.com" prompts seen at the top of the
> broken chat came from **multiple session files** being concatenated by `project_sessions_dir`.
> Segments 5–6 (RPC) collapse a chat to a single session file, which removes that duplication.
> Do not try to dedup session files in this segment — only fix error ordering.

## Build / restart
```bash
cd /home/p/VIDOEGAME/crack/_docker && ./run.sh
```

## Verify

### 1. Unit tests green + a new ordering test
```bash
docker exec crack-dev bash -lc \
  'cd /workspace/.pi/crack/server && PYTHONPATH=tests:. poetry run python -m pytest -q'
```
`tests/test_trajectory_view.py` exists — add a test that feeds `merge_exchange_sidecars` a
`projected` list with interleaved `timestamp`s and an `exchanges` list whose `errors[]` have `at`
values that fall **between** those timestamps, and assert the returned row order is strictly by
time (an error with `at` between turn A and turn B appears between them, not at the end).

### 2. Live: refresh the broken-style chat and eyeball order
Create a chat that will error a couple of times mid-run (or reuse any chat that has both turns and
error rows), then fetch the rendered trajectory and confirm error rows are interleaved, not bottom-
clustered:
```bash
CID=<some chat id with turns + errors>
curl -s "http://localhost:9847/chats/$CID/status" | grep -oE 'traj-(error|turn|user_prompt|annotation)' | head -40
```
**PASS:** `traj-error` rows appear **between** `traj-turn` rows in time order, not all bunched after
the last turn.

## Done when
Error rows render in timestamp order interleaved with turns/prompts, verified by a unit test in
`test_trajectory_view.py` and by inspecting a live chat's rendered trajectory.
