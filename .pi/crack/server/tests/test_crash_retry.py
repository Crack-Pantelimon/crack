"""Plan 24: crash mid-turn must retry (esp. sandbox where returncode is None)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from crack_server import paths, pi_proc, pi_runner, ratelimit
from tests.test_plan41 import fake_pi  # noqa: F401


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
    """pi exits without agent_end → crashed → retry resumes → agent_end."""
    monkeypatch.setattr(ratelimit, "HARD_RETRY_DELAYS", [0.01, 0.01])
    fake_pi.set_script(["die:1", "turns:1"])
    pid_file = tmp_path / "agent.pid"
    turns: list[dict] = []
    errors: list[dict] = []
    reason = pi_runner.run_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=__import__("time").monotonic(),
        persist_turn=lambda t, h: turns.append(dict(t)),
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert fake_pi.invocations() == 2
    assert len(errors) == 1
    assert "crashed" in errors[0]["message"]
    # Unique per-attempt output files; manifest points at the successful one.
    manifest = pi_proc._read_hop_manifest(paths.hop_manifest_path(pid_file))
    assert manifest["status"] == "done"
    out = Path(manifest["output_path"])
    assert out.name.endswith(".hop.1.1.jsonl") or ".hop.1.1." in out.name
    assert out.is_file()


def test_sandbox_style_crash_with_none_returncode_retries(monkeypatch, tmp_path):
    """Even with returncode None (sandbox), crashed=True must not clean-exit."""
    monkeypatch.setattr(ratelimit, "HARD_RETRY_DELAYS", [0.01])
    calls = {"n": 0}

    async def fake_once(p, attempt_idx, attempt_message):
        calls["n"] += 1
        if calls["n"] == 1:
            return {
                "reason": "crashed",
                "terminated_by_us": False,
                "terminal": False,
                "returncode": None,  # sandbox
                "persisted": 1,
                "ended_in_error": None,
                "crashed": True,
                "detail": "last output:\npartial",
            }
        return {
            "reason": "agent_end",
            "terminated_by_us": False,
            "terminal": True,
            "returncode": None,
            "persisted": 1,
            "ended_in_error": None,
            "crashed": False,
            "detail": "",
        }

    monkeypatch.setattr(pi_proc, "_attempt_once", fake_once)
    monkeypatch.setattr(pi_proc, "_live_detached_manifest", lambda p: None)
    errors: list[dict] = []
    p = pi_proc._HopParams(
        log_prefix="test",
        model="m",
        session_id="s",
        sessions_dir=tmp_path / "sessions",
        tools=None,
        start=__import__("time").monotonic(),
        sentinel=None,
        timeout_seconds=60,
        persist_turn=lambda t, h: None,
        hop=1,
        pid_file=tmp_path / "agent.pid",
        stop_check=None,
        env_extra=None,
        waiting_check=None,
        sandbox="crack-sbx-x",
    )

    async def _run():
        return await pi_proc._run_hop_with_retries(
            p, "hi", record_error=lambda e: errors.append(e) or len(errors),
        )

    reason = __import__("asyncio").run(_run())
    assert reason == "agent_end"
    assert calls["n"] == 2
    assert errors and "crashed" in errors[0]["message"]


def test_structured_error_preferred_over_generic(monkeypatch, tmp_path):
    monkeypatch.setattr(ratelimit, "HARD_RETRY_DELAYS", [0.01])
    monkeypatch.setattr(ratelimit, "MAX_TOTAL_ERRORS", 1)
    calls = {"n": 0}

    async def fake_once(p, attempt_idx, attempt_message):
        calls["n"] += 1
        return {
            "reason": "crashed",
            "terminated_by_us": False,
            "terminal": False,
            "returncode": None,
            "persisted": 0,
            "ended_in_error": "Bad Gateway 502 from provider",
            "crashed": True,
            "detail": "last stderr:\nnoise",
        }

    monkeypatch.setattr(pi_proc, "_attempt_once", fake_once)
    monkeypatch.setattr(pi_proc, "_live_detached_manifest", lambda p: None)
    errors: list[dict] = []
    p = pi_proc._HopParams(
        log_prefix="test",
        model="m",
        session_id="s",
        sessions_dir=tmp_path / "sessions",
        tools=None,
        start=__import__("time").monotonic(),
        sentinel=None,
        timeout_seconds=60,
        persist_turn=lambda t, h: None,
        hop=1,
        pid_file=tmp_path / "agent.pid",
        stop_check=None,
        env_extra=None,
        waiting_check=None,
    )

    async def _run():
        with pytest.raises(pi_proc.PiError) as ei:
            await pi_proc._run_hop_with_retries(
                p, "hi",
                record_error=lambda e: errors.append(e) or len(errors),
                error_budget=lambda: 1,
            )
        return ei.value

    err = __import__("asyncio").run(_run())
    assert "Bad Gateway 502" in str(err)
    assert errors and "Bad Gateway 502" in errors[0]["message"]


def test_nul_junk_lines_dropped():
    sink = pi_proc._StreamSink(pi_proc._HopParams(
        log_prefix="t", model="m", session_id="s",
        sessions_dir=Path("/tmp"), tools=None,
        start=0.0, sentinel=None, timeout_seconds=60,
        persist_turn=lambda t, h: None, hop=1, pid_file=None,
        stop_check=None, env_extra=None, waiting_check=None,
    ))
    assert pi_proc._process_stream_line(sink, "\x00\x00\x00", lambda: None) is False
    assert sink.stderr_tail == []
    assert sink.output_tail == []


def test_attempt_output_paths_are_unique(tmp_path):
    p = pi_proc._HopParams(
        log_prefix="t", model="m", session_id="s",
        sessions_dir=tmp_path / "sessions", tools=None,
        start=0.0, sentinel=None, timeout_seconds=60,
        persist_turn=lambda t, h: None, hop=3,
        pid_file=tmp_path / "agent.pid",
        stop_check=None, env_extra=None, waiting_check=None,
    )
    a0 = pi_proc._attempt_output_path(p, 0)
    a1 = pi_proc._attempt_output_path(p, 1)
    assert a0 != a1
    assert a0.name == "agent.hop.3.0.jsonl"
    assert a1.name == "agent.hop.3.1.jsonl"
