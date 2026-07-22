"""Unit tests for crack_server.pi_rpc (fake RPC subprocess)."""

from __future__ import annotations

import asyncio
import sys
import time
from pathlib import Path

import pytest

from crack_server import pi_rpc

FAKE_RPC = Path(__file__).parent / "fake_pi_rpc.py"


def _hop_kwargs(tmp_path, **over):
    kwargs = {
        "log_prefix": "test",
        "model": "moonshotai/x",
        "session_id": "rpc-hop",
        "sessions_dir": tmp_path / "sessions",
        "tools": "bash",
        "message": "do it",
        "start": time.monotonic(),
        "sentinel": None,
        "timeout_seconds": 60,
        "sandbox": None,
    }
    kwargs.update(over)
    return kwargs


async def _fake_launch(argv, *, sandbox, env):
    del argv, sandbox, env
    return await asyncio.create_subprocess_exec(
        sys.executable,
        str(FAKE_RPC),
        "normal",
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )


@pytest.mark.anyio
async def test_rpc_persists_turn_and_returns_agent_end(tmp_path, monkeypatch):
    monkeypatch.setattr(pi_rpc, "_launch_rpc_proc", _fake_launch)
    turns: list[dict] = []
    reason = await pi_rpc.arun_agent_hop_rpc(
        **_hop_kwargs(tmp_path),
        persist_turn=lambda t, h: turns.append(dict(t)),
    )
    assert reason == "agent_end"
    assert len(turns) == 1
    assert "hello from rpc fake" in turns[0].get("text", "")


@pytest.mark.anyio
async def test_rpc_stop_check_sends_abort_and_returns_stopped(tmp_path, monkeypatch):
    monkeypatch.setattr(pi_rpc, "_launch_rpc_proc", _fake_launch)

    async def launch_abort(argv, *, sandbox, env):
        return await asyncio.create_subprocess_exec(
            sys.executable,
            str(FAKE_RPC),
            "abort",
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

    monkeypatch.setattr(pi_rpc, "_launch_rpc_proc", launch_abort)

    checks = {"n": 0}

    def stop_check():
        checks["n"] += 1
        return checks["n"] >= 2

    reason = await pi_rpc.arun_agent_hop_rpc(
        **_hop_kwargs(tmp_path),
        persist_turn=lambda t, h: None,
        stop_check=stop_check,
    )
    assert reason == "stopped"
    assert checks["n"] >= 2
