"""RPC safety-net retries and exact error surfacing."""

from __future__ import annotations

import asyncio
import sys
import time
from pathlib import Path

import pytest

from crack_server import pi_proc, pi_rpc, pi_runner, ratelimit
from tests.test_plan41 import FAKE_RPC, _fake_rpc_launch, fake_pi  # noqa: F401


def _hop_kwargs(tmp_path, pid_file, **over):
    kwargs = {
        "log_prefix": "test",
        "model": "moonshotai/x",
        "session_id": "hop-test",
        "sessions_dir": tmp_path / "sessions",
        "tools": "bash",
        "message": "do it",
        "sentinel": None,
        "timeout_seconds": 60,
        "pid_file": pid_file,
    }
    kwargs.update(over)
    return kwargs


def test_die_mid_turn_retries_then_succeeds(fake_pi, tmp_path, monkeypatch):
    monkeypatch.setattr(pi_rpc, "RPC_SAFETY_BACKOFF_SECONDS", 0.01)
    fake_pi.set_script(["die:1", "turns:1"])
    pid_file = tmp_path / "agent.pid"
    turns: list[dict] = []
    errors: list[dict] = []
    reason = pi_runner.run_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: turns.append(dict(t)),
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert fake_pi.invocations() == 2
    assert len(errors) == 1
    assert "exited unexpectedly" in errors[0]["message"]
    assert fake_pi.prompt(2) == pi_runner.RESUME_MESSAGE


def test_structured_error_preferred_over_stderr(fake_pi, tmp_path, monkeypatch):
    monkeypatch.setattr(ratelimit, "MAX_TOTAL_ERRORS", 1)
    fake_pi.set_script(["autoretryfail:0"])
    errors: list[dict] = []
    with pytest.raises(pi_proc.PiError) as ei:
        pi_runner.run_agent_hop(
            **_hop_kwargs(tmp_path, tmp_path / "agent.pid"),
            start=time.monotonic(),
            persist_turn=lambda t, h: None,
            record_error=lambda e: errors.append(e) or len(errors),
            error_budget=lambda: 1,
        )
    assert "429 status code" in str(ei.value)
    assert errors[0]["detail"] == "429 status code (no body)"


@pytest.mark.anyio
async def test_prompt_rejection_surfaces_exact_detail(tmp_path, monkeypatch):
    async def launch(argv, *, sandbox, env):
        del argv, sandbox, env
        return await asyncio.create_subprocess_exec(
            sys.executable, str(FAKE_RPC), "promptreject",
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

    monkeypatch.setattr(pi_rpc, "_launch_rpc_proc", launch)
    errors: list[dict] = []
    with pytest.raises(pi_proc.PiError) as ei:
        await pi_rpc.arun_agent_hop_rpc(
            log_prefix="test",
            model="moonshotai/x",
            session_id="rpc-hop",
            sessions_dir=tmp_path / "sessions",
            tools="bash",
            message="do it",
            start=time.monotonic(),
            sentinel=None,
            timeout_seconds=60,
            persist_turn=lambda t, h: None,
            record_error=lambda e: errors.append(e) or len(errors),
        )
    assert ei.value.detail == "No project session found"
    assert errors[0]["detail"] == "No project session found"
