"""B13: PiError.detail must surface the raw stderr tail, not just JSON events.

The streaming hop merges stderr into stdout, where the JSON-event parser
consumes it; the old output_tail then held only well-formed events and the
crash-explaining stderr survived only as a truncated WARN log line. The fix
keeps a separate raw stderr ring buffer and prefers it when composing
PiError.detail.
"""

from __future__ import annotations

import os
import shutil
import time
from pathlib import Path

import pytest

from crack_server import pi_proc, ratelimit

SHIM = Path(__file__).parent / "fake_pi.sh"


@pytest.fixture
def fake_pi_script(tmp_path, monkeypatch) -> Path:
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
    # Fast retry schedules so failure paths finish in well under a second each.
    monkeypatch.setattr(ratelimit, "TRANSIENT_RETRY_DELAYS", [0.05, 0.05, 0.05])
    monkeypatch.setattr(ratelimit, "PI_RETRY_WINDOW_SECONDS", 0.2)
    return script


def run_hop(tmp_path, message="do it"):
    return pi_proc.run_agent_hop(
        log_prefix="test",
        model="moonshotai/x",
        session_id="b13-test",
        sessions_dir=tmp_path / "sessions",
        tools="bash",
        message=message,
        start=time.monotonic(),
        sentinel=None,
        timeout_seconds=60,
        persist_turn=lambda t, h: None,
    )


def test_compose_detail_prefers_stderr_and_labels():
    detail = pi_proc._compose_detail('{"type":"turn_end"}', "boom: it broke")
    assert detail == "last stderr:\nboom: it broke"
    # No stderr: fall back to the output (JSON-event) tail, labeled as such.
    assert pi_proc._compose_detail('{"type":"turn_end"}', "") == \
        'last output:\n{"type":"turn_end"}'
    assert pi_proc._compose_detail("", "") == ""


def test_hop_hard_failure_detail_contains_stderr(fake_pi_script, tmp_path):
    fake_pi_script.write_text("hard\n", encoding="utf-8")
    with pytest.raises(pi_proc.PiError) as excinfo:
        run_hop(tmp_path)
    detail = excinfo.value.detail
    assert "boom: unrecoverable parse explosion" in detail
    assert detail.startswith("last stderr:")


def test_hop_midfail_detail_prefers_stderr_over_json_events(fake_pi_script, tmp_path):
    # Two well-formed JSON turns stream, then a transient death on stderr: the
    # detail must show the stderr line, not (only) the JSON event tail.
    fake_pi_script.write_text("midfail:2\n", encoding="utf-8")
    with pytest.raises(pi_proc.PiError) as excinfo:
        run_hop(tmp_path)
    detail = excinfo.value.detail
    assert "connection reset by peer" in detail
    assert detail.startswith("last stderr:")


def test_run_pi_text_hard_failure_detail_contains_stderr(fake_pi_script):
    fake_pi_script.write_text("hard\n", encoding="utf-8")
    with pytest.raises(pi_proc.PiError) as excinfo:
        pi_proc.run_pi_text("hello", log_prefix="t", model="moonshotai/x")
    detail = excinfo.value.detail
    assert "boom: unrecoverable parse explosion" in detail
    assert detail.startswith("last stderr:")
