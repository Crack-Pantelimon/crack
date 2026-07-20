"""Async worker smoke tests: hops from different chats genuinely interleave
(one asyncio process, no thread cap), and the queue wakeup hook fires."""

from __future__ import annotations

import asyncio
import os
import shutil
import time
from pathlib import Path

import pytest

from crack_server import paths, queue, ratelimit, worker
from tests.test_plan41 import FakePi, SHIM


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


def _make_chat(tmp_path, monkeypatch, fake_pi, suffix: str) -> str:
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    # Numeric suffix keeps the id valid (CHAT_ID_RE) and unique within a ms.
    chat_id = f"{paths.generate_chat_id()}_{suffix}"
    paths.create_chat(chat_id, "moonshotai/x")

    def _titled(info: dict) -> dict:
        info["title"] = f"chat {suffix}"  # skip the (async) title-gen pi call
        return info

    paths.chat_info_state(chat_id).update(_titled)

    def _begin(state: dict) -> dict:
        state.setdefault("pending", []).append({"user": f"hello {suffix}", "source": "human"})
        state["phase"] = "chatting"
        return state

    paths.chat_state(chat_id).update(_begin)
    return chat_id


@pytest.mark.anyio
async def test_two_chat_hops_interleave(tmp_path, monkeypatch, fake_pi):
    """Two 2s-sleeping chat hops dispatched concurrently finish in ~2s, not ~4s."""
    import crack_server.app  # noqa: F401  (app↔stages import cycle, mirrors worker)
    from crack_server import chats

    fake_pi.set_script(["sleepy:2"])
    chat_a = _make_chat(tmp_path, monkeypatch, fake_pi, "1")
    chat_b = _make_chat(tmp_path, monkeypatch, fake_pi, "2")

    queue.enqueue(chat_a, chats.CHAT_JOB_SLUG, "chat")
    queue.enqueue(chat_b, chats.CHAT_JOB_SLUG, "chat")
    job_a, job_b = queue.claim_next(), queue.claim_next()
    assert job_a is not None and job_b is not None

    start = time.monotonic()
    await asyncio.gather(worker._dispatch(job_a), worker._dispatch(job_b))
    elapsed = time.monotonic() - start

    for chat_id in (chat_a, chat_b):
        state = paths.chat_state(chat_id).read()
        assert state.get("phase") == "idle", state.get("error")
        exchanges = state.get("exchanges", [])
        assert len(exchanges) == 1
        assert exchanges[0]["turns"], "expected a persisted turn"
    # Sequential execution would take >= 4s of shim sleep alone.
    assert elapsed < 3.5, f"hops did not interleave (took {elapsed:.1f}s)"


@pytest.mark.anyio
async def test_enqueue_fires_wakeup_callback(tmp_path, monkeypatch, fake_pi):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    fired: list[str] = []
    queue.register_wakeup(lambda: fired.append("wake"))
    try:
        queue.enqueue("t1", "explore", "run")
    finally:
        queue._WAKEUP_CALLBACKS.clear()
    assert fired == ["wake"]
