"""Sub-agent spawn/run/resume/nudge tests against fake_pi.sh (single coder)."""

from __future__ import annotations

import json
import os
import shutil
import time
from pathlib import Path

import pytest

from crack_server import chats, paths, pi_rpc, queue, ratelimit, worker
from crack_server.sub_agents import MAX_DEPTH, MAX_PARALLEL_SUBAGENTS, runner
from crack_server.sub_agents import registry as sub_registry
from tests.test_plan41 import FakePi, SHIM, _fake_rpc_launch

REAL_PERSONAS = Path(__file__).resolve().parents[2] / "sub_agents"


def _seed_personas(root: Path) -> None:
    dest = root / ".pi" / "crack" / "sub_agents"
    if dest.exists():
        shutil.rmtree(dest)
    assert REAL_PERSONAS.is_dir(), f"missing checked-in personas at {REAL_PERSONAS}"
    shutil.copytree(REAL_PERSONAS, dest)


async def _drain_jobs(max_jobs: int = 50) -> int:
    """Claim and dispatch pending jobs until empty (or max_jobs)."""
    n = 0
    while n < max_jobs:
        job = queue.claim_next()
        if job is None:
            break
        await worker._dispatch(job)
        n += 1
    return n


@pytest.fixture
def fake_pi(tmp_path, monkeypatch) -> FakePi:
    bin_dir = tmp_path / "fakebin"
    bin_dir.mkdir()
    target = bin_dir / "pi"
    shutil.copy(SHIM, target)
    target.chmod(0o755)
    ctrl = tmp_path / "fakepi-ctrl"
    ctrl.mkdir()
    script = tmp_path / "fakepi-script"
    monkeypatch.setenv("PATH", f"{bin_dir}:{os.environ['PATH']}")
    monkeypatch.setenv("FAKE_PI_DIR", str(ctrl))
    monkeypatch.setenv("FAKE_PI_SCRIPT", str(script))
    monkeypatch.setattr(pi_rpc, "_launch_rpc_proc", _fake_rpc_launch)
    monkeypatch.setattr(pi_rpc, "RPC_SAFETY_BACKOFF_SECONDS", 0.01)
    monkeypatch.setattr(ratelimit, "TRANSIENT_RETRY_DELAYS", [0.05, 0.05, 0.05])
    monkeypatch.setattr(ratelimit, "HARD_RETRY_DELAYS", [0.05, 0.05, 0.05, 0.05])
    monkeypatch.setattr(ratelimit, "PI_RETRY_WINDOW_SECONDS", 0.2)
    monkeypatch.setattr(ratelimit, "NVIDIA_CALLS_PER_MINUTE", 1_000_000.0)
    monkeypatch.setattr(ratelimit, "_provider_limiters", {})
    monkeypatch.setattr(ratelimit, "_model_limiters", {})
    return FakePi(ctrl, script)


@pytest.fixture
def chat_root(tmp_path, monkeypatch, fake_pi) -> str:
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    _seed_personas(tmp_path)
    sub_registry.clear_cache()
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, "nvidia/z-ai/glm-5.2")
    return chat_id


def test_personas_discovered(chat_root):
    slugs = [p.slug for p in sub_registry.list_personas()]
    assert slugs == ["coder"]


def test_active_child_count_endpoint(chat_root):
    from crack_server.routes_sub_agents import api_active_child_count

    assert api_active_child_count(chat_root, "chat", chat_root) == {"active": 0}

    runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="busy",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    assert api_active_child_count(chat_root, "chat", chat_root) == {"active": 1}

    run_id = paths.list_run_ids(chat_root)[0]
    runner.finish(run_id, "done")
    assert api_active_child_count(chat_root, "chat", chat_root) == {"active": 0}



@pytest.mark.anyio
async def test_spawn_run_report_parent_resume(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report", "turns:1"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="Investigate the foo module.",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = state["run_id"]
    assert state["depth"] == 1
    assert Path(state["report_path"]).name == "report.md"

    n = await _drain_jobs()
    assert n >= 1

    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "done"
    assert Path(run["report_path"]).is_file()

    # Parent chat should have been resumed with a child_report exchange.
    chat = paths.chat_state(chat_root).read()
    assert chat.get("child_inbox") in (None, [])
    sources = [e.get("source") for e in chat.get("exchanges", [])]
    assert "child_report" in sources



@pytest.mark.anyio
async def test_nudge_then_report(chat_root, fake_pi):
    # First hop: settle with no tools and no report → nudge; second: write report.
    fake_pi.set_script(["ok", "turns:1", "write_report"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="Implement X.",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = state["run_id"]
    await _drain_jobs()
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "done"
    assert run["nudge_count"] >= 1
    assert Path(run["report_path"]).is_file()



@pytest.mark.anyio
async def test_nudge_exhaustion_errors_and_resumes_parent(chat_root, fake_pi):
    fake_pi.set_script(["ok", "turns:1"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="Test Y.",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = state["run_id"]
    await _drain_jobs(max_jobs=20)
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "error"
    assert run["nudge_count"] >= 3
    chat = paths.chat_state(chat_root).read()
    assert any(e.get("source") == "child_report" for e in chat.get("exchanges", []))



@pytest.mark.anyio
async def test_depth_limit_rejects_spawn_beyond_max(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    parent = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="L1",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    await _drain_jobs()
    # Manually set depth to MAX_DEPTH so a further spawn is rejected.
    def _max(s: dict) -> dict:
        s["depth"] = MAX_DEPTH
        s["phase"] = "running"
        s["children"] = []
        s.pop("parent_notified", None)
        s["finished_at"] = None
        return s

    paths.run_state(chat_root, parent["run_id"]).update(_max)
    with pytest.raises(ValueError, match="exceeds maximum"):
        runner.spawn(
            chat_id=chat_root,
            persona_slug="coder",
            instructions="too deep",
            parent_kind="run",
            parent_id=parent["run_id"],
            depth=MAX_DEPTH,
        )



@pytest.mark.anyio
async def test_parallel_children_both_delivered(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    a = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="A",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    b = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="B",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    await _drain_jobs()
    chat = paths.chat_state(chat_root).read()
    report_exchanges = [
        e for e in chat.get("exchanges", []) if e.get("source") == "child_report"
    ]
    assert len(report_exchanges) >= 2
    joined = "\n".join(e.get("user", "") for e in report_exchanges)
    assert a["run_id"] in joined
    assert b["run_id"] in joined



@pytest.mark.anyio
async def test_reclaim_orphans_requeues(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="resume me",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    job = queue.claim_next()
    assert job is not None
    # Simulate crash: leave job in processing, reclaim it.
    assert Path(job["_path"]).is_file()
    # Age the processing file so reclaim picks it up.
    os.utime(job["_path"], (0, 0))
    n = queue.reclaim_orphans(threshold_seconds=0.0)
    assert n >= 1
    await _drain_jobs()
    run = paths.run_state(chat_root, state["run_id"]).read()
    assert run["phase"] == "done"



def test_api_list_personas(chat_root):
    from crack_server.routes_sub_agents import api_list_sub_agents

    data = api_list_sub_agents()
    assert {p["slug"] for p in data} == {"coder"}
    assert all("tool_name" in p for p in data)



@pytest.mark.anyio
async def test_api_spawn(chat_root, fake_pi):
    from starlette.requests import Request
    from crack_server.routes_sub_agents import api_spawn_sub_agent

    fake_pi.set_script(["ok", "write_report", "turns:1"])

    body = json.dumps({
        "persona": "coder",
        "instructions": "look around",
        "parent_kind": "chat",
        "parent_id": chat_root,
        "depth": 0,
    }).encode()

    scope = {
        "type": "http",
        "method": "POST",
        "path": f"/api/chats/{chat_root}/sub_agents/spawn",
        "headers": [(b"content-type", b"application/json")],
    }

    async def receive():
        return {"type": "http.request", "body": body, "more_body": False}

    request = Request(scope, receive)
    response = await api_spawn_sub_agent(chat_root, request)
    assert response.status_code == 200
    payload = json.loads(response.body)
    assert "run_id" in payload and "report_path" in payload
    await _drain_jobs()
    run = paths.run_state(chat_root, payload["run_id"]).read()
    assert run["phase"] == "done"


def test_sidebar_tree_always_polls(chat_root):
    html = chats.render_sidebar_tree(chat_root)
    assert 'hx-trigger="every 2s"' in html


def test_sidebar_tree_shows_spawned_run(chat_root):
    runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="Sidebar test",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = paths.list_run_ids(chat_root)[0]
    html = chats.render_sidebar_tree(chat_root)
    assert run_id in html


@pytest.mark.anyio
async def test_run_gets_title_on_begin(chat_root, fake_pi):
    fake_pi.set_script(["ok", "write_report"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="Title me",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    await _drain_jobs()
    run = paths.run_state(chat_root, state["run_id"]).read()
    assert run.get("title") == "text-response"


def test_sidebar_node_shows_title_not_persona(chat_root):
    run_id = paths.generate_run_id()
    paths.run_dir(chat_root, run_id).mkdir(parents=True)
    paths.run_state(chat_root, run_id).write({
        "run_id": run_id,
        "persona": "coder",
        "title": "Fix the widget",
        "chat_id": chat_root,
        "parent_kind": "chat",
        "parent_id": chat_root,
        "depth": 1,
        "phase": "running",
        "hops_completed": 2,
        "created_at": time.time() - 360,
    })
    html = chats.render_sidebar_tree(chat_root)
    assert "Fix the widget" in html
    assert 'title="coder"' in html


def test_sidebar_order_and_metrics(chat_root, monkeypatch):
    now = time.time()
    older = paths.generate_run_id()
    newer = paths.generate_run_id()
    while newer <= older:
        newer = paths.generate_run_id()
    for run_id, created, finished, hops, phase in (
        (older, now - 600, now - 60, 3, "done"),
        (newer, now - 120, None, 1, "running"),
    ):
        paths.run_dir(chat_root, run_id).mkdir(parents=True)
        state = {
            "run_id": run_id,
            "persona": "coder",
            "chat_id": chat_root,
            "parent_kind": "chat",
            "parent_id": chat_root,
            "depth": 1,
            "phase": phase,
            "hops_completed": hops,
            "created_at": created,
        }
        if finished is not None:
            state["finished_at"] = finished
        paths.run_state(chat_root, run_id).write(state)

    html = chats.render_sidebar_tree(chat_root)
    assert html.index("#1") < html.index("#2")
    assert "3 turns" in html
    assert "ran for" in html
    assert "running for" in html


def test_fill_template_depth_gating(chat_root):
    persona = sub_registry.get("coder")
    assert persona is not None
    shallow = {"instructions": "x", "report_path": "/p", "depth": 0}
    deep = {"instructions": "x", "report_path": "/p", "depth": MAX_DEPTH}
    text0 = persona._fill_template("system.md", shallow)
    text1 = persona._fill_template("system.md", deep)
    assert "spawn_coder" in text0
    assert "wait_join" in text0
    assert "spawn_coder" not in text1
    assert "wait_join" not in text1
    assert "{sub_agent_instructions}" not in text1


@pytest.mark.anyio
async def test_spawn_parallel_cap_slot_pending(chat_root, monkeypatch):
    from crack_server.routes_sub_agents import api_spawn_sub_agent
    from starlette.requests import Request

    monkeypatch.setattr(
        "crack_server.routes_sub_agents.SPAWN_BLOCK_SECONDS", 0.0, raising=False
    )

    for i in range(MAX_PARALLEL_SUBAGENTS):
        runner.spawn(
            chat_id=chat_root,
            persona_slug="coder",
            instructions=f"blocker {i}",
            parent_kind="chat",
            parent_id=chat_root,
            depth=0,
        )

    body = json.dumps({
        "persona": "coder",
        "instructions": "fourth",
        "parent_kind": "chat",
        "parent_id": chat_root,
        "depth": 0,
    }).encode()

    scope = {
        "type": "http",
        "method": "POST",
        "path": f"/api/chats/{chat_root}/sub_agents/spawn",
        "headers": [(b"content-type", b"application/json")],
    }

    async def receive():
        return {"type": "http.request", "body": body, "more_body": False}

    request = Request(scope, receive)
    blocked = await api_spawn_sub_agent(chat_root, request)
    assert json.loads(blocked.body) == {"status": "slot_pending"}


@pytest.mark.anyio
async def test_spawn_waits_for_free_slot(chat_root, fake_pi):
    import asyncio

    from crack_server.routes_sub_agents import api_spawn_sub_agent
    from starlette.requests import Request

    for i in range(MAX_PARALLEL_SUBAGENTS):
        runner.spawn(
            chat_id=chat_root,
            persona_slug="coder",
            instructions=f"blocker {i}",
            parent_kind="chat",
            parent_id=chat_root,
            depth=0,
        )

    body = json.dumps({
        "persona": "coder",
        "instructions": "fourth",
        "parent_kind": "chat",
        "parent_id": chat_root,
        "depth": 0,
    }).encode()

    scope = {
        "type": "http",
        "method": "POST",
        "path": f"/api/chats/{chat_root}/sub_agents/spawn",
        "headers": [(b"content-type", b"application/json")],
    }

    async def receive():
        return {"type": "http.request", "body": body, "more_body": False}

    request = Request(scope, receive)
    task = asyncio.create_task(api_spawn_sub_agent(chat_root, request))
    await asyncio.sleep(0.1)
    runner.finish(paths.list_run_ids(chat_root)[0], "done")
    response = await asyncio.wait_for(task, timeout=5)
    payload = json.loads(response.body)
    assert "run_id" in payload
    assert payload.get("waited") is True
