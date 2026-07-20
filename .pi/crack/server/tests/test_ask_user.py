"""ask_user tests: hop-terminating suspension for run parents (no nudge, no
successor, orphan-sweep skip), resume-with-answer hops, and the chat-parent
record-the-question path."""

from __future__ import annotations

import asyncio
import json
import os
import time

import pytest

from crack_server import paths, queue, worker
from crack_server.sub_agents import ask_user, runner
from crack_server.sub_agents import registry as sub_registry
from tests.test_sub_agents import _drain_jobs, _seed_personas, chat_root, fake_pi  # noqa: F401  (fixtures)
from tests.test_wait_join import _json_request


@pytest.mark.anyio
async def test_ask_user_suspends_run_then_answer_resumes(chat_root, fake_pi):
    # Hop 1 sleeps so the test can call ask_user mid-hop (as the tool would);
    # hop 2 (the answer resume) writes the report.
    fake_pi.set_script(["sleepy:1", "write_report"])
    state = runner.spawn(
        chat_id=chat_root, persona_slug="explorer", instructions="investigate",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    job = queue.claim_next()
    assert job is not None
    dispatch = asyncio.create_task(worker._dispatch(job))
    await asyncio.sleep(0.3)  # hop is running inside the sleepy shim

    status = ask_user.ask(
        chat_id=chat_root, parent_kind="run", parent_id=run_id,
        question="Which color?", choices=["red", "blue"],
    )
    assert status == "awaiting_user"

    await asyncio.wait_for(dispatch, timeout=10)
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "awaiting_user"
    assert run["pending_question"]["question"] == "Which color?"
    assert run.get("nudge_count", 0) == 0
    # Hop-terminating: no successor/nudge job was enqueued.
    assert queue.claim_next() is None

    # The answer resumes the run with a hop that receives the answer text.
    assert ask_user.answer(chat_root, run_id, "blue") is True
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "resuming"
    assert "pending_question" not in run
    assert run["user_qa"][-1]["answer"] == "blue"

    await _drain_jobs()
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "done", run.get("error")
    prompt = fake_pi.prompt(2)
    assert "Question: Which color?" in prompt
    assert "Answer: blue" in prompt


@pytest.mark.anyio
async def test_ask_user_orphan_sweep_skips_awaiting_user(chat_root, fake_pi):
    state = runner.spawn(
        chat_id=chat_root, persona_slug="explorer", instructions="wait",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    queue.complete(queue.claim_next())  # drop run_start: no job behind the run

    ask_user.ask(
        chat_id=chat_root, parent_kind="run", parent_id=run_id,
        question="overnight question",
    )
    state_obj = paths.run_state(chat_root, run_id)
    old = time.time() - 3600  # overnight-style: hours old, no queued job
    os.utime(state_obj.path, (old, old))

    persona = sub_registry.get("explorer")
    assert persona.check_orphaned(run_id) is False
    assert state_obj.read()["phase"] == "awaiting_user"
    # The sweep itself must also leave it alone.
    worker._sweep_orphaned_phases()
    assert state_obj.read()["phase"] == "awaiting_user"


@pytest.mark.anyio
async def test_ask_user_answer_requires_awaiting_phase(chat_root, fake_pi):
    state = runner.spawn(
        chat_id=chat_root, persona_slug="explorer", instructions="x",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    queue.complete(queue.claim_next())
    assert ask_user.answer(chat_root, state["run_id"], "too early") is False
    # Terminal runs cannot be asked.
    runner.finish(state["run_id"], "done")
    with pytest.raises(ValueError, match="terminal"):
        ask_user.ask(
            chat_id=chat_root, parent_kind="run", parent_id=state["run_id"],
            question="too late",
        )


@pytest.mark.anyio
async def test_ask_user_route_and_chat_parent(chat_root, fake_pi):
    from fastapi import HTTPException
    from crack_server import chats
    from crack_server.routes_sub_agents import api_ask_user

    # Chat parent: question is recorded, no phase games.
    request = _json_request(
        {"parent_kind": "chat", "parent_id": chat_root,
         "question": "Proceed with the risky option?", "choices": ["yes", "no"]},
        f"/api/chats/{chat_root}/ask_user",
    )
    response = await api_ask_user(chat_root, request)
    assert json.loads(response.body)["status"] == "recorded"
    state = paths.chat_state(chat_root).read()
    assert state["pending_question"]["question"] == "Proceed with the risky option?"

    # The chat page renders the question prominently…
    tail = chats.render_chat_tail(chat_root)
    assert "Proceed with the risky option?" in tail

    # …and the chat's normal input is the answer channel.
    chats.post_message(chat_root, "yes, proceed", None)
    state = paths.chat_state(chat_root).read()
    assert "pending_question" not in state
    assert state["pending"][-1]["user"] == "yes, proceed"
    queue.complete(queue.claim_next())  # drop the chat job; pi not needed here

    # Validation: no question → 400.
    bad = _json_request(
        {"parent_kind": "chat", "parent_id": chat_root, "question": ""},
        f"/api/chats/{chat_root}/ask_user",
    )
    with pytest.raises(HTTPException) as excinfo:
        await api_ask_user(chat_root, bad)
    assert excinfo.value.status_code == 400


@pytest.mark.anyio
async def test_user_answer_route(chat_root, fake_pi):
    from fastapi import HTTPException
    from crack_server.routes_sub_agents import api_run_user_answer

    state = runner.spawn(
        chat_id=chat_root, persona_slug="explorer", instructions="x",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = state["run_id"]
    queue.complete(queue.claim_next())
    ask_user.ask(
        chat_id=chat_root, parent_kind="run", parent_id=run_id, question="q?",
    )

    from starlette.requests import Request

    scope = {"type": "http", "method": "POST", "headers": []}
    request = Request(scope)
    response = await api_run_user_answer(chat_root, run_id, request, answer="sure")
    assert response.status_code == 303  # plain form: redirect back to the run page
    run = paths.run_state(chat_root, run_id).read()
    assert run["phase"] == "resuming"
    job = queue.claim_next()
    assert job is not None and job["step"] == "run"
    assert "Answer: sure" in job["form"]["user_answer"]
    queue.complete(job)

    # Not awaiting anymore → 409.
    with pytest.raises(HTTPException) as excinfo:
        await api_run_user_answer(chat_root, run_id, request, answer="again")
    assert excinfo.value.status_code == 409
