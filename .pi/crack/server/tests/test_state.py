"""JsonState (state.py) unit tests.

The critical property (B3): ``update`` is a read-modify-write under a per-path
flock, so concurrent writers — threads in the web process AND the separate
worker process — never silently revert each other's fields. These tests hammer
one state file from threads and from multiple OS processes and assert every
writer's increments/appends survive.
"""

from __future__ import annotations

import multiprocessing as mp
import shutil
import threading
from pathlib import Path

from crack_server.state import JsonState

INCREMENTS = 100


def _increment(state: JsonState, key: str, n: int) -> None:
    """Bump ``state[key]`` n times, each via its own locked update cycle."""
    for _ in range(n):
        def bump(data: dict) -> dict:
            data[key] = data.get(key, 0) + 1
            data.setdefault(f"{key}_log", []).append(data[key])
            return data

        state.update(bump)


def _process_worker(path: str, key: str, n: int) -> None:
    """Separate-process writer (module-level so it is picklable/spawn-safe)."""
    _increment(JsonState(Path(path)), key, n)


def test_read_tolerant_of_missing_and_corrupt(tmp_path):
    state = JsonState(tmp_path / "state.json")
    assert state.read() == {}
    (tmp_path / "state.json").write_text("{not json", encoding="utf-8")
    assert state.read() == {}
    (tmp_path / "state.json").write_text("[1, 2]", encoding="utf-8")
    assert state.read() == {}


def test_write_and_read_roundtrip(tmp_path):
    state = JsonState(tmp_path / "state.json")
    state.write({"a": 1, "b": [2, 3]})
    assert state.read() == {"a": 1, "b": [2, 3]}
    # Atomic write went through rename — no tmp file left behind.
    assert not (tmp_path / "state.json.tmp").exists()


def test_write_skips_when_parent_dir_is_gone(tmp_path, caplog):
    """B7: a straggler write must not resurrect a deleted task/chat dir."""
    victim = tmp_path / "deleted_task"
    victim.mkdir()
    state = JsonState(victim / "explore.json")
    state.write({"status": "running"})
    assert state.read() == {"status": "running"}

    shutil.rmtree(victim)  # task deleted while the "worker" is still running
    with caplog.at_level("WARNING"):
        state.write({"status": "done"})
        state.update(lambda d: {**d, "status": "done"})
    assert not victim.exists(), "write must not recreate the deleted dir"
    assert "refusing to write" in caplog.text


def test_update_loses_no_fields_under_threads(tmp_path):
    state = JsonState(tmp_path / "state.json")
    keys = [f"thread_{i}" for i in range(4)]
    threads = [
        threading.Thread(target=_increment, args=(state, key, INCREMENTS))
        for key in keys
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    final = state.read()
    for key in keys:
        assert final[key] == INCREMENTS
        assert final[f"{key}_log"] == list(range(1, INCREMENTS + 1))


def test_update_loses_no_fields_across_processes(tmp_path):
    path = str(tmp_path / "state.json")
    keys = [f"proc_{i}" for i in range(3)]
    ctx = mp.get_context("fork")
    procs = [
        ctx.Process(target=_process_worker, args=(path, key, INCREMENTS))
        for key in keys
    ]
    for p in procs:
        p.start()
    for p in procs:
        p.join(timeout=30)
        assert p.exitcode == 0

    final = JsonState(Path(path)).read()
    for key in keys:
        assert final[key] == INCREMENTS
        assert final[f"{key}_log"] == list(range(1, INCREMENTS + 1))


def test_update_loses_no_fields_threads_and_processes_mixed(tmp_path):
    """The real deployment shape: web-process threads + a separate worker process."""
    path = tmp_path / "state.json"
    state = JsonState(path)

    ctx = mp.get_context("fork")
    workers = [
        ctx.Process(target=_process_worker, args=(str(path), f"proc_{i}", INCREMENTS))
        for i in range(2)
    ]
    threads = [
        threading.Thread(target=_increment, args=(state, f"thread_{i}", INCREMENTS))
        for i in range(2)
    ]
    for p in workers:
        p.start()
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    for p in workers:
        p.join(timeout=30)
        assert p.exitcode == 0

    final = state.read()
    for prefix in ("proc_0", "proc_1", "thread_0", "thread_1"):
        assert final[prefix] == INCREMENTS
        assert final[f"{prefix}_log"] == list(range(1, INCREMENTS + 1))
