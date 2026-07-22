"""STOP must stay latched until a human message or explicit retry resumes the chat."""

from __future__ import annotations

import pytest

from crack_server import chat_engine, chats, paths, patch, queue
from crack_server.sub_agents import registry, runner
from tests.test_sub_agents import chat_root, fake_pi  # noqa: F401  (fixtures)


@pytest.fixture
def noop_enqueue(monkeypatch):
    monkeypatch.setattr(queue, "enqueue_exclusive", lambda *a, **k: "job-id")


def test_stop_chat_sets_stop_requested(chat_root, monkeypatch):
    monkeypatch.setattr(chats.pi_runner, "kill_pid_file", lambda _p: False)
    chats.stop_chat(chat_root)
    assert paths.chat_state(chat_root).read()["stop_requested"] is True


def test_pop_pending_drains_queue_while_stopped(chat_root):
    chat = paths.chat_state(chat_root)

    def _seed(s: dict) -> dict:
        s["stop_requested"] = True
        s["pending"] = [{"user": "queued", "source": "system"}]
        return s

    chat.update(_seed)
    assert chats._pop_pending(chat_root) is None
    state = chat.read()
    assert state["stop_requested"] is True
    assert state["pending"] == []


def test_enqueue_system_message_preserves_stop(chat_root, noop_enqueue):
    chat = paths.chat_state(chat_root)
    chat.update(lambda s: {**s, "stop_requested": True})
    patch.enqueue_chat_system_message(chat_root, "internal nag", source="patch_guard")
    state = chat.read()
    assert state["stop_requested"] is True
    assert state["pending"]


def test_merge_child_inbox_preserves_stop(chat_root, noop_enqueue):
    chat = paths.chat_state(chat_root)

    def _seed(s: dict) -> dict:
        s["stop_requested"] = True
        s["child_inbox"] = [{
            "run_id": "child-1",
            "persona": "coder",
            "status": "done",
            "last_message": "done",
            "report_excerpt": "",
            "report_path": "",
        }]
        return s

    chat.update(_seed)
    assert chats._merge_child_inbox(chat_root) == 1
    state = chat.read()
    assert state["stop_requested"] is True
    assert state["pending"]


def test_post_message_clears_stop(chat_root, noop_enqueue):
    chat = paths.chat_state(chat_root)
    chat.update(lambda s: {**s, "stop_requested": True})
    chats.post_message(chat_root, "resume please", model=None)
    assert paths.chat_state(chat_root).read()["stop_requested"] is False


def test_answer_chat_question_clears_stop(chat_root, noop_enqueue):
    chat = paths.chat_state(chat_root)
    chat.update(lambda s: {
        **s,
        "stop_requested": True,
        "pending_question": {"question": "Pick one", "choices": ["a", "b"]},
    })
    chats.answer_chat_question(chat_root, "a")
    assert paths.chat_state(chat_root).read()["stop_requested"] is False


@pytest.mark.anyio
async def test_exchange_finish_preserves_stop_requested(chat_root):
    chat = paths.chat_state(chat_root)
    sessions_dir = paths.chat_sessions_dir(chat_root)
    sessions_dir.mkdir(parents=True, exist_ok=True)
    chat.update(lambda s: {
        **s,
        "stop_requested": True,
        "exchanges": [{"user": "hi", "turns": []}],
    })

    await chat_engine.run_exchange(
        state=chat,
        ident=chat_root,
        message_builder=lambda user_msg: user_msg,
        record_template="",
        log_prefix="test-stop",
        model="nvidia/z-ai/glm-5.2",
        session_id=f"unscripted-{chat_root}",
        sessions_dir=sessions_dir,
        tools=None,
        timeout_seconds=60,
        pre_stop_check=lambda: True,
    )

    state = chat.read()
    assert state["stop_requested"] is True
    assert state["phase"] == "idle"


def test_subagent_stop_does_not_clear_parent_stop(chat_root, fake_pi, monkeypatch):
    monkeypatch.setattr(chats.pi_runner, "kill_pid_file", lambda _p: False)
    chat = paths.chat_state(chat_root)
    chat.update(lambda s: {**s, "stop_requested": True})

    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="work",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    persona = registry.get("coder")
    assert persona is not None
    persona.request_stop(state["run_id"])

    assert paths.chat_state(chat_root).read()["stop_requested"] is True
    assert paths.run_state(chat_root, state["run_id"]).read()["stop_requested"] is True


def test_subagent_retry_clears_only_run_stop(chat_root, fake_pi, monkeypatch):
    monkeypatch.setattr(queue, "enqueue_exclusive", lambda *a, **k: "job-id")
    chat = paths.chat_state(chat_root)
    chat.update(lambda s: {**s, "stop_requested": True})

    state = runner.spawn(
        chat_id=chat_root,
        persona_slug="coder",
        instructions="work",
        parent_kind="chat",
        parent_id=chat_root,
        depth=0,
    )
    run_id = state["run_id"]
    paths.run_state(chat_root, run_id).update(lambda s: {
        **s,
        "phase": "error",
        "stop_requested": True,
    })

    persona = registry.get("coder")
    assert persona is not None
    persona.retry(run_id)

    assert paths.chat_state(chat_root).read()["stop_requested"] is True
    assert paths.run_state(chat_root, run_id).read()["stop_requested"] is False
