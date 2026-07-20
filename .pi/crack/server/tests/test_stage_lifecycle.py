"""Stage-level acceptance checks for plan 4.1, run against the fake pi shim:

- §1: an Explore run records one ``user_prompt`` entry per pi invocation whose
  ``compiled`` text matches the shim's received prompt byte-for-byte;
- §5: explore retry-from-error resumes the session instead of restarting;
- §6: the generic ``message`` action clears error state and enqueues a resume;
- §7: a stale start job (token mismatch) is dropped by the worker dispatch.
"""

from __future__ import annotations

import os
import shutil
import time
from pathlib import Path

import pytest

import crack_server.app  # noqa: F401  (must load before stages — app↔stages import cycle)
from crack_server import paths, queue, ratelimit

SHIM = Path(__file__).parent / "fake_pi.sh"

from tests.test_plan41 import FakePi  # reuse the shim controller


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
    # Neutralize the nvidia limiters so stage runs don't pace the test suite.
    monkeypatch.setattr(ratelimit, "NVIDIA_CALLS_PER_MINUTE", 1_000_000.0)
    monkeypatch.setattr(ratelimit, "_provider_limiters", {})
    monkeypatch.setattr(ratelimit, "_model_limiters", {})
    return FakePi(ctrl, script)


@pytest.fixture
def task(tmp_path, monkeypatch, fake_pi) -> str:
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    task_id = "1000000000000_test_task"
    paths.task_dir(task_id).mkdir(parents=True)
    paths.write_prompt(task_id, "prompt.md", "Build the thing.")
    return task_id


def explore_stage():
    from crack_server import stages

    stage = stages.get("explore")
    assert stage is not None
    return stage


def test_explore_run_records_compiled_prompts(task, fake_pi):
    stage = explore_stage()
    # 1: turn-zero (print), 2: hop 1 ends on sentinel, 3: summary (print).
    fake_pi.set_script(["ok", "sentinel:EXPLORATION_COMPLETE", "ok"])
    stage._run_job(task)

    state = paths.explore_state(task).read()
    assert state["status"] == "done"
    assert state["stop_reason"] == "sentinel"

    entries = [t for t in state["turns"] if t.get("kind") == "user_prompt"]
    assert len(entries) == 3, "one prompt entry per pi invocation"
    by_label = {e["label"]: e for e in entries}
    assert by_label["turn_zero"]["compiled"] == fake_pi.prompt(1)
    assert by_label["hop 1"]["compiled"] == fake_pi.prompt(2)
    assert by_label["summary"]["compiled"] == fake_pi.prompt(3)
    assert by_label["turn_zero"]["template"] == "turn_zero.md"
    assert by_label["hop 1"]["template"] == "explore.md"
    assert by_label["summary"]["template"] == "explore_summary.md"
    # Assistant turn dicts stay kind-less so old renderers keep working.
    assert all("kind" not in t for t in state["turns"] if t.get("text") is not None
               and not t.get("kind"))


def test_explore_retry_resumes_session(task, fake_pi):
    stage = explore_stage()
    # First run: turn-zero ok, then the hop dies hard with no progress on all
    # 5 attempts (the no-progress streak cap) — the stage lands in error with
    # each failed attempt recorded as a durable error row. Retry: the hop
    # resumes (invocation 7) and ends on the sentinel, then the summary runs.
    fake_pi.set_script(["ok", "hard", "hard", "hard", "hard", "hard",
                        "sentinel:EXPLORATION_COMPLETE", "ok"])
    stage._run_job(task)

    state = paths.explore_state(task).read()
    assert state["status"] == "error"
    kept = [t for t in state["turns"] if not t.get("kind")]
    assert kept == []
    assert len(state["errors"]) == 1 + len(ratelimit.HARD_RETRY_DELAYS)
    assert state["error_over_budget"] is False

    stage.retry_from_error(task)
    st = paths.explore_state(task).read()
    assert st["status"] == "running"
    # A manual continue grants another MAX_TOTAL_ERRORS on top of the rows so
    # far; the durable rows themselves are kept.
    assert st["error_budget"] == len(st["errors"]) + ratelimit.MAX_TOTAL_ERRORS
    job = queue.claim_next()
    assert job is not None and job["slug"] == "explore" and job["step"] == "resume"
    stage.dispatch_step(job["task_id"], job["step"], job.get("form"))
    queue.complete(job)

    state = paths.explore_state(task).read()
    assert state["status"] == "done"
    # The resume hop's turn was kept, the resume prompt was sent, and both
    # hops used the same pi session id.
    kept = [t for t in state["turns"] if not t.get("kind")]
    assert len(kept) == 1
    assert fake_pi.prompt(7) == "Continue exploring where you left off."
    a2, a7 = fake_pi.argv(2), fake_pi.argv(7)
    assert a2[a2.index("--session-id") + 1] == a7[a7.index("--session-id") + 1]
    assert (paths.explore_sessions_dir(task)).is_dir()


def test_message_action_clears_error_and_enqueues_resume(task, fake_pi):
    stage = explore_stage()
    state = paths.explore_state(task).read()
    state.update(
        {"status": "error", "error": "boom", "error_detail": "tail", "questions": ["q"]}
    )
    paths.explore_state(task).write(state)

    stage.post_user_message(task, {"msg": "look at the tests too"})
    st = paths.explore_state(task).read()
    assert st["status"] == "running"
    assert st["error"] == "" and st["error_detail"] == ""
    assert st["stop_requested"] is False

    job = queue.claim_next()
    assert job is not None and job["step"] == "user_message"
    assert job["form"]["msg"] == "look at the tests too"

    # The queued step drives the hop with the user's text as the message.
    fake_pi.set_script(["sentinel:EXPLORATION_COMPLETE", "ok"])
    stage.dispatch_step(job["task_id"], job["step"], job.get("form"))
    queue.complete(job)
    assert fake_pi.prompt(1) == "look at the tests too"
    assert paths.explore_state(task).read()["status"] == "done"


def test_stale_start_job_dropped_by_token(task, fake_pi):
    stage = explore_stage()
    fake_pi.set_script(["ok"])
    stage.start(task)
    job = queue.claim_next()
    assert job is not None and job["form"]["started_token"]

    # A newer start overwrote the token before the worker ran the stale job.
    st = paths.explore_state(task).read()
    st["started_token"] = "newer-token"
    paths.explore_state(task).write(st)

    stage.dispatch_step(job["task_id"], job["step"], job.get("form"))
    queue.complete(job)
    assert fake_pi.invocations() == 0, "stale job must exit before any pi call"


def test_double_start_enqueues_exactly_one_job(task, fake_pi):
    stage = explore_stage()
    stage.start(task)
    # Second POST: the status=="running" guard stops it, and even a racing
    # start would be dropped by enqueue_exclusive.
    stage.start(task)
    jobs = []
    while (job := queue.claim_next()) is not None:
        jobs.append(job)
        queue.complete(job)
    assert len(jobs) == 1
