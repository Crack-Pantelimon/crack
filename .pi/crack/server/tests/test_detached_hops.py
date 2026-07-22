"""Reload survival: orphaned pid files are reaped; sessions resume via RPC."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from crack_server import paths, pi_runner, worker


def _write_meta(pid_file: Path, *, sandbox: str, session_id: str) -> None:
    meta = pid_file.with_name(pid_file.stem + ".meta.json")
    meta.write_text(
        f'{{"sandbox": "{sandbox}", "session_id": "{session_id}"}}',
        encoding="utf-8",
    )


def test_recover_detached_hops_kills_orphan_pid_files(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))

    def make_pid_file(cid):
        directory = paths.chat_dir(cid)
        directory.mkdir(parents=True)
        return directory / "agent.pid"

    live = subprocess.Popen(
        ["python3", "-c", "import time; time.sleep(5)"],
        start_new_session=True,
    )
    pid_file = make_pid_file("1000000000001")
    pid_file.write_text(str(live.pid), encoding="utf-8")

    try:
        worker.recover_detached_hops()
        assert not pid_file.exists()
    finally:
        live.kill()
        live.wait()


def test_kill_pid_file_uses_meta_for_sandbox(tmp_path, monkeypatch):
    """Sandbox STOP uses the meta sidecar, not hop manifests."""
    from crack_server import pi_proc

    calls: list[tuple[str, str]] = []

    def fake_kill(sbx, sid):
        calls.append((sbx, sid))

    monkeypatch.setattr(
        "crack_server.sandbox.kill_session_sync", fake_kill,
    )
    pid_file = tmp_path / "agent.pid"
    pid_file.write_text("99999", encoding="utf-8")
    _write_meta(pid_file, sandbox="crack-sbx-test", session_id="unscripted-x")
    pi_proc.kill_pid_file(pid_file)
    assert calls == [("crack-sbx-test", "unscripted-x")]
