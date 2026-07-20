"""Detached-hop reload survival: pi keeps running across a server reload and
the restarted worker re-attaches instead of killing/respawning.

Covers: CancelledError detaches instead of killing (manifest stays "running",
pid_file survives), re-attach tails from the stored offset and persists only
new turns without a second pi, a pi that finished mid-restart is drained to
completion from the output file alone, and recover_detached_hops leaves live
hops alone while cleaning up dead/stale ones.
"""

from __future__ import annotations

import asyncio
import json
import signal
import subprocess
import time

import pytest

from crack_server import paths, pi_proc, pi_runner, worker
from tests.test_plan41 import fake_pi  # noqa: F401  (fixture)


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


async def _wait_for(cond, timeout=10.0):
    deadline = time.monotonic() + timeout
    while not cond():
        assert time.monotonic() < deadline, "timed out waiting"
        await asyncio.sleep(0.05)


@pytest.mark.anyio
async def test_cancel_detaches_pi_instead_of_killing(fake_pi, tmp_path):
    fake_pi.set_script(["sleepy:30"])
    pid_file = tmp_path / "agent.pid"
    turns: list[dict] = []
    task = asyncio.create_task(pi_proc.arun_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: turns.append(dict(t)),
    ))
    await _wait_for(pid_file.exists)
    pid = int(pid_file.read_text())
    await _wait_for(lambda: len(turns) == 1)

    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task

    # pi survived the "reload": still alive, manifest left status=running,
    # pid_file kept so a later STOP or re-attach can find the pid.
    assert pi_proc._pid_alive(pid, "hop-test")
    assert pid_file.exists()
    manifest = pi_proc._read_hop_manifest(paths.hop_manifest_path(pid_file))
    assert manifest["status"] == "running"
    assert manifest["pid"] == pid
    assert manifest["offset"] > 0  # the persisted turn's offset was flushed

    assert pi_runner.kill_pid_file(pid_file)
    pid_file.unlink(missing_ok=True)


@pytest.mark.anyio
async def test_reattach_tails_detached_pi_without_respawning(fake_pi, tmp_path):
    fake_pi.set_script(["turnsgap:3:2"])
    pid_file = tmp_path / "agent.pid"

    turns1: list[dict] = []
    task = asyncio.create_task(pi_proc.arun_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: turns1.append(dict(t)),
    ))
    await _wait_for(pid_file.exists)
    await _wait_for(lambda: len(turns1) == 1)
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task

    # Re-attach: same session, live manifest → tails from the stored offset,
    # persists only the turns emitted after the detach, and never spawns a
    # second pi for the session.
    turns2: list[dict] = []
    reason = await pi_proc.arun_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: turns2.append(dict(t)),
    )
    assert reason == "agent_end"
    assert [t["text"] for t in turns2] == ["turn 2 (invocation 1)", "turn 3 (invocation 1)"]
    assert fake_pi.invocations() == 1
    assert not pid_file.exists()  # reaped by the re-attach
    manifest = pi_proc._read_hop_manifest(paths.hop_manifest_path(pid_file))
    assert manifest["status"] == "done"


def test_reattach_drains_backlog_when_pi_finished_during_restart(fake_pi, tmp_path):
    fake_pi.set_script(["turns:2"])
    # Simulate a pi that ran to completion while the worker was down: run the
    # shim directly, stdout redirected to the hop output file, manifest left
    # status=running with offset 0.
    pid_file = tmp_path / "agent.pid"
    output_path = tmp_path / "agent.hop.jsonl"
    argv = ["pi", "--mode", "json", "-p", "--model", "moonshotai/x",
            "--session-id", "hop-test", "--session-dir", str(tmp_path / "sessions"), "do it"]
    with open(output_path, "wb") as out:
        proc = subprocess.Popen(argv, stdout=out, stderr=subprocess.STDOUT)
        dead_pid = proc.pid
        assert proc.wait() == 0
    (tmp_path / "agent.hop.json").write_text(json.dumps({
        "pid": dead_pid,
        "started_at": time.time(),
        "output_path": str(output_path),
        "offset": 0,
        "session_id": "hop-test",
        "model": "moonshotai/x",
        "tools": "bash",
        "message": "do it",
        "hop": 1,
        "timeout": 60,
        "status": "running",
    }))

    turns: list[dict] = []
    reason = pi_runner.run_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: turns.append(dict(t)),
    )
    # The backlog held the terminal event: the hop completed from the file
    # alone — both turns persisted, no second pi spawned.
    assert reason == "agent_end"
    assert len(turns) == 2
    assert fake_pi.invocations() == 1


def _write_manifest(pid_file, **fields):
    manifest = {
        "pid": 0,
        "started_at": time.time(),
        "output_path": "",
        "offset": 0,
        "session_id": "s",
        "timeout": 60,
        "status": "running",
    }
    manifest.update(fields)
    paths.hop_manifest_path(pid_file).write_text(json.dumps(manifest), encoding="utf-8")


def test_recover_detached_hops(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))

    def make_pid_file(tid):
        directory = paths.task_dir(tid)
        directory.mkdir(parents=True)
        return directory / "explore.agent.pid"

    # 1. Live, fresh detached hop → left running for re-attach. (python3 keeps
    # its argv — including the fake session id — in /proc/<pid>/cmdline.)
    live = subprocess.Popen(
        ["python3", "-c", "import time; time.sleep(30)", "s-live"],
        start_new_session=True,
    )
    pid_file1 = make_pid_file("t1")
    pid_file1.write_text(str(live.pid), encoding="utf-8")
    _write_manifest(pid_file1, pid=live.pid, session_id="s-live")

    # 2. Dead pid, fresh manifest → left for the resumed job to drain.
    dead = subprocess.Popen(["true"])
    dead.wait()
    pid_file2 = make_pid_file("t2")
    pid_file2.write_text(str(dead.pid), encoding="utf-8")
    _write_manifest(pid_file2, pid=dead.pid)

    # 3. Dead pid, stale manifest → cleaned up.
    pid_file3 = make_pid_file("t3")
    pid_file3.write_text(str(dead.pid), encoding="utf-8")
    _write_manifest(pid_file3, pid=dead.pid, started_at=time.time() - 10000)

    # 4. Stale pid file with no manifest → legacy kill + unlink.
    pid_file4 = make_pid_file("t4")
    pid_file4.write_text(str(dead.pid), encoding="utf-8")

    try:
        worker.recover_detached_hops()

        assert paths.hop_manifest_path(pid_file1).is_file()
        assert pid_file1.is_file()
        assert pi_proc._pid_alive(live.pid, "s-live")
        assert paths.hop_manifest_path(pid_file2).is_file()
        assert not pid_file3.exists()
        assert not paths.hop_manifest_path(pid_file3).exists()
        assert not pid_file4.exists()
    finally:
        live.kill()
        live.wait()


# ---------------------------------------------------------------------------
# Detached-pid ledger: bound grace-period detaches across attempts
# ---------------------------------------------------------------------------


def test_grace_detach_records_detached_pids_before_retry(fake_pi, tmp_path, monkeypatch):
    # Empty agent_end + linger past EXIT_GRACE → detached_pids entry lands in
    # the manifest before the successful second attempt spawns.
    monkeypatch.setattr(pi_proc, "EXIT_GRACE_SECONDS", 0.3)
    fake_pi.set_script(["detach:2", "turns:1"])
    pid_file = tmp_path / "agent.pid"
    seen: list[list[dict]] = []
    orig_write = pi_proc._write_hop_manifest

    def capture(path, data):
        entries = data.get("detached_pids") or []
        if entries:
            seen.append(list(entries))
        return orig_write(path, data)

    monkeypatch.setattr(pi_proc, "_write_hop_manifest", capture)
    errors: list[dict] = []
    reason = pi_runner.run_agent_hop(
        **_hop_kwargs(tmp_path, pid_file),
        start=time.monotonic(),
        persist_turn=lambda t, h: None,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert fake_pi.invocations() == 2
    assert len(errors) == 1
    assert errors[0]["message"] == "pi returned only empty turns"
    assert seen, "expected at least one manifest write with detached_pids"
    assert all("pid" in e and "since" in e for batch in seen for e in batch)


def test_sweep_detached_pids_sigterms_then_sigkills(monkeypatch):
    now = 1_000_000.0
    monkeypatch.setattr(pi_proc.time, "time", lambda: now)
    monkeypatch.setattr(pi_proc, "DETACHED_TERMINATE_AFTER_SECONDS", 10)
    monkeypatch.setattr(pi_proc, "DETACHED_KILL_AFTER_SECONDS", 5)

    alive = {111, 222, 333}
    monkeypatch.setattr(
        pi_proc, "_pid_alive",
        lambda pid, session_id=None: pid in alive,
    )
    signals: list[tuple[int, int]] = []

    def fake_terminate(pid, sig):
        signals.append((pid, sig))
        if sig == signal.SIGKILL:
            alive.discard(pid)

    monkeypatch.setattr(pi_proc, "_terminate_group", fake_terminate)

    entries = [
        {"pid": 111, "since": now - 5},   # young: keep
        {"pid": 222, "since": now - 15},  # past terminate: SIGTERM + stamp
        {"pid": 333, "since": now - 40, "sigterm_at": now - 6},  # past kill
        {"pid": 444, "since": now - 100},  # dead: drop
    ]
    survivors = pi_proc._sweep_detached_pids(entries, "hop-test", "test", 1)
    assert signals == [(222, signal.SIGTERM), (333, signal.SIGKILL)]
    assert [e["pid"] for e in survivors] == [111, 222]
    assert survivors[1]["sigterm_at"] == now


def test_kill_pid_file_also_kills_detached_pids(tmp_path):
    # Current pid + two ledger entries; kill_pid_file must reap all three.
    current = subprocess.Popen(["sleep", "60"], start_new_session=True)
    detached_a = subprocess.Popen(["sleep", "60"], start_new_session=True)
    detached_b = subprocess.Popen(["sleep", "60"], start_new_session=True)
    pid_file = tmp_path / "agent.pid"
    pid_file.write_text(str(current.pid), encoding="utf-8")
    pi_proc._write_hop_manifest(paths.hop_manifest_path(pid_file), {
        "pid": current.pid,
        "detached_pids": [
            {"pid": detached_a.pid, "since": time.time() - 10},
            {"pid": detached_b.pid, "since": time.time() - 20},
        ],
    })
    try:
        assert pi_proc.kill_pid_file(pid_file)
        # Brief wait for SIGKILL delivery / reap.
        for proc in (current, detached_a, detached_b):
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=1)
                raise AssertionError(f"pid {proc.pid} still alive after kill_pid_file")
        assert not pi_proc._pid_alive(current.pid)
        assert not pi_proc._pid_alive(detached_a.pid)
        assert not pi_proc._pid_alive(detached_b.pid)
    finally:
        for proc in (current, detached_a, detached_b):
            if proc.poll() is None:
                proc.kill()
                proc.wait()
