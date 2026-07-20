"""Sub-agent spawn/run/resume/nudge/planner tests against fake_pi.sh."""

from __future__ import annotations

import json
import os
import shutil
from pathlib import Path

import pytest

from crack_server import paths, queue, ratelimit, worker
from crack_server.sub_agents import MAX_DEPTH, runner
from crack_server.sub_agents import registry as sub_registry
from tests.test_plan41 import FakePi, SHIM

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
    assert slugs == ["coder", "explorer", "planner", "tester"]



@pytest.mark.anyio
async def test_spawn_run_report_parent_resume(chat_root, fake_pi):
    fake_pi.set_script(["write_report", "turns:1"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="explorer",
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
    fake_pi.set_script(["turns:1", "write_report"])
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
    fake_pi.set_script(["turns:1"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="tester",
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
    fake_pi.set_script(["write_report"])
    parent = runner.spawn(
        chat_id=chat_root,
        persona_slug="explorer",
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
            persona_slug="explorer",
            instructions="too deep",
            parent_kind="run",
            parent_id=parent["run_id"],
            depth=MAX_DEPTH,
        )



@pytest.mark.anyio
async def test_parallel_children_both_delivered(chat_root, fake_pi):
    fake_pi.set_script(["write_report"])
    a = runner.spawn(
        chat_id=chat_root,
        persona_slug="explorer",
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
    fake_pi.set_script(["write_report"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="explorer",
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



@pytest.mark.anyio
async def test_planner_qa_round_then_write(chat_root, fake_pi):
    # grill → questions; after answers+continue → write_report
    fake_pi.set_script(["questions", "write_report"])
    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="planner",
        instructions="Plan the feature.",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = state["run_id"]
    await _drain_jobs()
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "awaiting_answers"
    assert run.get("pending_questions")

    persona = sub_registry.get("planner")
    assert persona is not None

    class _Form(dict):
        def getlist(self, key):
            v = self.get(key)
            if v is None:
                return []
            if isinstance(v, list):
                return v
            return [v]

    persona.submit_answers(run_id, _Form(q1="A"))
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "resuming"
    await _drain_jobs()

    # After followup with no new questions (script line repeats write_report? —
    # second invocation is write_report only if we continue_to_write).
    # questions behavior on followup would ask again; force continue.
    run = paths.run_state(chat_root, run_id).read()
    if run["phase"] == "awaiting_answers":
        persona.continue_to_write(run_id)
        await _drain_jobs()
    elif run["phase"] not in ("done", "writing"):
        persona.continue_to_write(run_id)
        await _drain_jobs()

    run = paths.run_state(chat_root, run_id).read()
    # Followup may have consumed write_report; ensure done or drive write.
    if run["phase"] != "done":
        fake_pi.set_script(["write_report"])
        persona.continue_to_write(run_id)
        await _drain_jobs()
        run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "done"
    assert Path(run["report_path"]).is_file()
    assert run.get("rounds")


def test_api_list_personas(chat_root):
    from crack_server.routes_sub_agents import api_list_sub_agents

    data = api_list_sub_agents()
    assert {p["slug"] for p in data} == {"coder", "explorer", "planner", "tester"}
    assert all("tool_name" in p for p in data)



@pytest.mark.anyio
async def test_api_spawn(chat_root, fake_pi):
    from starlette.requests import Request
    from crack_server.routes_sub_agents import api_spawn_sub_agent

    fake_pi.set_script(["write_report", "turns:1"])

    body = json.dumps({
        "persona": "explorer",
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
