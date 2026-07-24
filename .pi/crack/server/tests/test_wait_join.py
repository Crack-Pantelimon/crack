"""wait_join server-side tests: wait.poll target resolution, inbox draining
without duplicate delivery, the finish() notify gap, rebuilds, and the
long-poll route (signal wakeup + waiting_on stamping)."""

from __future__ import annotations

import asyncio
import json
import os
import time
from pathlib import Path

import pytest

from crack_server import paths, queue, worker
from crack_server.sub_agents import runner, wait
from crack_server.sub_agents.constants import SUBAGENT_JOB_SLUG
from crack_server.sub_agents import registry as sub_registry
from tests.test_sub_agents import _seed_personas, chat_root, fake_pi  # noqa: F401  (fixtures)

# Re-exported fixtures keep this file short; _seed_personas/FakePi live in
# test_sub_agents / test_plan41.


async def _drain_subagent_jobs(max_jobs: int = 50) -> list[dict]:
    """Dispatch only SUBAGENT jobs; non-subagent jobs are held in processing
    (simulating a queued-but-not-yet-run drain job) and returned."""
    held: list[dict] = []
    n = 0
    while n < max_jobs:
        job = queue.claim_next()
        if job is None:
            break
        if job.get("slug") == SUBAGENT_JOB_SLUG:
            await worker._dispatch(job)
            n += 1
        else:
            held.append(job)
    return held


def _json_request(body: dict, path: str):
    from starlette.requests import Request

    raw = json.dumps(body).encode()
    scope = {
        "type": "http",
        "method": "POST",
        "path": path,
        "headers": [(b"content-type", b"application/json")],
    }

    async def receive():
        return {"type": "http.request", "body": raw, "more_body": False}

    return Request(scope, receive)


@pytest.mark.anyio
async def test_wait_drains_chat_inbox_then_drain_job_noops(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    a = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="A",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    b = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="B",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    held = await _drain_subagent_jobs()
    assert held, "expected the chat drain_children job to be queued"
    for rid in (a["run_id"], b["run_id"]):
        assert paths.run_state(chat_root, rid).read()["phase"] == "done"
    assert len(paths.chat_state(chat_root).read().get("child_inbox") or []) == 2

    result = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root)
    assert result["pending"] == []
    assert {r["run_id"] for r in result["results"]} == {a["run_id"], b["run_id"]}
    assert all("finished with status: done" in r["text"] for r in result["results"])
    # Single consumption point: the inbox is empty now.
    assert paths.chat_state(chat_root).read().get("child_inbox") in (None, [])

    # The queued drain job now runs and must NOT produce a duplicate
    # child_report exchange (chats._merge_child_inbox no-ops on empty inbox).
    for job in held:
        await worker._dispatch(job)
    chat = paths.chat_state(chat_root).read()
    assert not [e for e in chat.get("exchanges", []) if e.get("source") == "child_report"]


async def _manual_run_child(
    chat_id: str, parent_run_id: str, *, instructions: str = "C"
) -> dict:
    """Create a child run under a run parent (runner.spawn rejects depth > MAX_DEPTH)."""
    import uuid

    from crack_server.sub_agents.constants import SUBAGENT_JOB_SLUG

    run_id = paths.generate_run_id()
    report_path = paths.run_report_path(chat_id, run_id).resolve()
    run_directory = paths.run_dir(chat_id, run_id)
    run_directory.mkdir(parents=True, exist_ok=True)
    (run_directory / "sessions").mkdir(parents=True, exist_ok=True)
    token = uuid.uuid4().hex
    now = time.time()
    state = {
        "run_id": run_id,
        "persona": "coder",
        "chat_id": chat_id,
        "parent_kind": "run",
        "parent_id": parent_run_id,
        "depth": 2,
        "instructions": instructions,
        "report_path": str(report_path),
        "plan": True,
        "phase": "running",
        "started_token": token,
        "stop_requested": False,
        "nudge_count": 0,
        "hops_completed": 0,
        "children": [],
        "turns": [],
        "child_inbox": [],
        "error": "",
        "finished_at": None,
        "created_at": now,
    }
    paths.run_state(chat_id, run_id).write(state)

    def _link_parent(s: dict) -> dict:
        children = list(s.get("children") or [])
        if run_id not in children:
            children.append(run_id)
        s["children"] = children
        return s

    paths.run_state_by_id(parent_run_id).update(_link_parent)
    queue.enqueue_exclusive(
        chat_id,
        SUBAGENT_JOB_SLUG,
        "run_start",
        {"run_id": run_id, "started_token": token},
        run_id=run_id,
    )
    return dict(state)


@pytest.mark.anyio
async def test_wait_run_parent_drain_no_duplicate_child_results(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    parent = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="P",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    child = await _manual_run_child(chat_root, parent["run_id"])
    # Run only the child's job; hold the parent's run_start and drain jobs.
    held: list[dict] = []
    for _ in range(10):
        job = queue.claim_next()
        if job is None:
            break
        if (job.get("run_id") or (job.get("form") or {}).get("run_id")) == child["run_id"]:
            await worker._dispatch(job)
        else:
            held.append(job)
    assert fake_pi.invocations() == 2

    parent_state = paths.run_state_by_id(parent["run_id"]).read()
    assert len(parent_state.get("child_inbox") or []) == 1
    assert parent_state.get("children") == []

    result = wait.poll(
        chat_id=chat_root, parent_kind="run", parent_id=parent["run_id"]
    )
    assert result["pending"] == []
    assert [r["run_id"] for r in result["results"]] == [child["run_id"]]
    assert paths.run_state_by_id(parent["run_id"]).read().get("child_inbox") in (None, [])

    # The held drain_children job no-ops: no resume hop with child_results.
    for job in held:
        if job.get("step") == "drain_children":
            await worker._dispatch(job)
    assert fake_pi.invocations() == 2, "drain job must not run a duplicate hop"


@pytest.mark.anyio
async def test_wait_target_resolution(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    a = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="A",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    b = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="B",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    held = await _drain_subagent_jobs()
    for job in held:
        queue.complete(job)  # drop the chat drain job; wait owns the inbox now

    # By run id: only A drains; B's entry stays.
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=a["run_id"])
    assert [r["run_id"] for r in res["results"]] == [a["run_id"]]
    assert res["pending"] == []
    assert len(paths.chat_state(chat_root).read().get("child_inbox") or []) == 1

    # Bogus target: nothing, and nothing pending.
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target="bogus")
    assert res["results"] == [] and res["pending"] == []

    # By persona slug: B drains.
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target="coder")
    assert [r["run_id"] for r in res["results"]] == [b["run_id"]]

    # A again (consumed just now, still within the notify gap): reported as
    # pending-notified so the caller's two-strike rule can escalate…
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=a["run_id"])
    assert res["results"] == []
    assert [p["run_id"] for p in res["pending"]] == [a["run_id"]]
    assert res["pending"][0]["notified"] is True

    # …and an explicit rebuild returns the entry, flagged delivered_earlier.
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=a["run_id"], rebuild=[a["run_id"]])
    assert [r["run_id"] for r in res["results"]] == [a["run_id"]]
    assert res["results"][0]["delivered_earlier"] is True

    # Past the gap window, a targeted poll on the consumed run rebuilds
    # immediately without a rebuild request.
    def _age(s: dict) -> dict:
        s["finished_at"] = time.time() - wait.NOTIFIED_GAP_SECONDS - 5
        return s

    paths.run_state(chat_root, a["run_id"]).update(_age)
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=a["run_id"])
    assert [r["run_id"] for r in res["results"]] == [a["run_id"]]
    assert res["results"][0]["delivered_earlier"] is True
    assert res["pending"] == []


@pytest.mark.anyio
async def test_notified_gap_not_misread_as_delivered(chat_root, fake_pi):
    """notified=true with no inbox entry (the finish() two-write gap) is
    pending, never silently treated as delivered."""
    state = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="G",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    queue.complete(queue.claim_next())  # drop run_start; handcraft the gap

    def _gap(s: dict) -> dict:
        s["phase"] = "done"
        s["parent_notified"] = True
        s["finished_at"] = time.time()
        return s

    paths.run_state(chat_root, run_id).update(_gap)
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root)
    assert res["results"] == []
    assert [p["run_id"] for p in res["pending"]] == [run_id]
    assert res["pending"][0]["notified"] is True


@pytest.mark.anyio
async def test_finish_in_progress_not_rebuilt_early(chat_root, fake_pi):
    """terminal + !notified inside FINISH_GAP must stay pending.

    Reproducing the harmful wait_join timing bug: phase stamped done before
    finish() completes extract/drain must not unblock the parent via a
    delivered_earlier rebuild.
    """
    state = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="F",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    queue.complete(queue.claim_next())

    def _mid_finish(s: dict) -> dict:
        s["phase"] = "done"
        s["parent_notified"] = False
        s["finished_at"] = time.time()
        return s

    paths.run_state(chat_root, run_id).update(_mid_finish)
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=run_id)
    assert res["results"] == []
    assert [p["run_id"] for p in res["pending"]] == [run_id]
    assert res["pending"][0]["notified"] is False

    # Past the finish gap → crash recovery rebuild is allowed.
    def _age(s: dict) -> dict:
        s["finished_at"] = time.time() - wait.FINISH_GAP_SECONDS - 5
        return s

    paths.run_state(chat_root, run_id).update(_age)
    res = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
                    target=run_id)
    assert [r["run_id"] for r in res["results"]] == [run_id]
    assert res["results"][0]["delivered_earlier"] is True
    assert res["pending"] == []


@pytest.mark.anyio
async def test_wait_route_long_poll_wakes_on_notify(chat_root, fake_pi):
    from crack_server.routes_sub_agents import api_wait_sub_agents

    state = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="W",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    request = _json_request(
        {"parent_kind": "chat", "parent_id": chat_root, "block_seconds": 5},
        f"/api/chats/{chat_root}/sub_agents/wait",
    )
    started = time.monotonic()
    task = asyncio.create_task(api_wait_sub_agents(chat_root, request))
    await asyncio.sleep(0.5)
    # While blocked, the parent state is stamped so the orphan sweep skips it.
    waiting = paths.chat_state(chat_root).read()
    assert waiting.get("waiting_on") == [run_id]
    assert waiting.get("waiting_since")

    runner.finish(run_id, "done")  # sets the signal after the inbox write
    response = await asyncio.wait_for(task, timeout=5)
    elapsed = time.monotonic() - started
    assert elapsed < 3.0, f"long-poll did not wake on notify ({elapsed:.1f}s)"
    payload = json.loads(response.body)
    assert [r["run_id"] for r in payload["results"]] == [run_id]
    assert payload["pending"] == []
    # waiting_on cleared on return.
    assert not paths.chat_state(chat_root).read().get("waiting_on")


@pytest.mark.anyio
async def test_wait_route_validation(chat_root, fake_pi):
    from fastapi import HTTPException
    from crack_server.routes_sub_agents import api_wait_sub_agents

    bad = _json_request(
        {"parent_kind": "bogus", "parent_id": chat_root},
        f"/api/chats/{chat_root}/sub_agents/wait",
    )
    with pytest.raises(HTTPException) as excinfo:
        await api_wait_sub_agents(chat_root, bad)
    assert excinfo.value.status_code == 400

    missing_run = _json_request(
        {"parent_kind": "run", "parent_id": "1000000000000_deadbeef"},
        f"/api/chats/{chat_root}/sub_agents/wait",
    )
    with pytest.raises(HTTPException) as excinfo:
        await api_wait_sub_agents(chat_root, missing_run)
    assert excinfo.value.status_code == 404


@pytest.mark.anyio
async def test_orphan_check_skips_waiting_parent(chat_root, fake_pi):
    state = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="S",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    queue.complete(queue.claim_next())  # no job behind the run at all

    def _waiting(s: dict) -> dict:
        s["waiting_on"] = ["1000000000000_somechild"]
        s["waiting_since"] = time.time()
        return s

    state_obj = paths.run_state(chat_root, run_id)
    state_obj.update(_waiting)
    old = time.time() - 3600
    os.utime(state_obj.path, (old, old))

    persona = sub_registry.get("coder")
    assert persona.check_orphaned(run_id) is False
    assert state_obj.read()["phase"] == "running"
