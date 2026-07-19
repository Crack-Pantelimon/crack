"""Plan/Plan-Review revamp tests: verified-artifact completion + queue chaining.

Covers the regressions the blender_mcp stall exposed:

- RC1: a stage's own successor step survives the real worker cycle
  (claim → dispatch → complete → deferred enqueue) instead of being dropped
  by the B1 exclusive guard;
- RC2: the write/revise steps verify final_plan.md on disk (exists, changed
  this step, required headings) and send named corrective retries;
- RC6: the orphan-phase watchdog flips "running phase, no job" into an error.
"""

from __future__ import annotations

import os
import shutil
import time
from pathlib import Path

import pytest

import crack_server.app  # noqa: F401  (must load before stages — app↔stages import cycle)
from crack_server import paths, queue, ratelimit, worker
from crack_server.stages.s02_plan import REQUIRED_PLAN_HEADINGS
from crack_server.stages.steprun import (
    file_content_hash,
    run_until_verified,
    verify_artifact_file,
)

SHIM = Path(__file__).parent / "fake_pi.sh"

from tests.test_plan41 import FakePi  # reuse the shim controller


VALID_PLAN = """# Plan

## Initial build/check instructions
run make

## Problem statement
stuff is broken

## Changes
change the things

## What NOT to change
everything else

## Automatic verification
pytest

## Manual verification
look at it

## Overview / Summary
short recap
"""


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
    monkeypatch.setattr(ratelimit, "PI_RETRY_WINDOW_SECONDS", 0.2)
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


def plan_stage():
    from crack_server import stages

    stage = stages.get("plan")
    assert stage is not None
    return stage


def review_stage():
    from crack_server import stages

    stage = stages.get("plan_review")
    assert stage is not None
    return stage


# ---------------------------------------------------------------------------
# RC1 — the real worker cycle chains draft → write (regression for the
# "queue: dropping duplicate plan/final" stall)
# ---------------------------------------------------------------------------


def test_draft_chains_to_write_through_worker_cycle(task, fake_pi, tmp_path):
    stage = plan_stage()
    plan_path = paths.plan_dir(task) / "final_plan.md"
    src = tmp_path / "plan_src.md"
    src.write_text(VALID_PLAN, encoding="utf-8")
    # 1: draft ends READY_TO_PLAN; 2: write agent "writes" the plan file;
    # 3: todo regeneration (print mode).
    fake_pi.set_script(
        ["sentinel:READY_TO_PLAN", f"copy:{src}>{plan_path}", "ok"]
    )

    stage.start(task)
    job = queue.claim_next()
    assert job is not None and (job["slug"], job["step"]) == ("plan", "draft")
    worker._dispatch(job)

    # The decisive RC1 assertion: the successor was enqueued, not dropped.
    write_job = queue.claim_next()
    assert write_job is not None
    assert (write_job["slug"], write_job["step"]) == ("plan", "write")
    assert paths.plan_state(task).read()["phase"] == "write_running"

    worker._dispatch(write_job)
    state = paths.plan_state(task).read()
    assert state["phase"] == "done", state.get("error")
    text = plan_path.read_text(encoding="utf-8")
    for heading in REQUIRED_PLAN_HEADINGS:
        assert heading in text
    assert state["final_md"] == VALID_PLAN
    # Todo was regenerated (single-shot) and Plan Review auto-started.
    assert (paths.plan_dir(task) / "todo.md").read_text(encoding="utf-8").strip() == "text-response"
    review_job = queue.claim_next()
    assert review_job is not None
    assert (review_job["slug"], review_job["step"]) == ("plan_review", "critique")


def test_write_step_corrective_retry_names_deficiency(task, fake_pi, tmp_path):
    stage = plan_stage()
    plan_path = paths.plan_dir(task) / "final_plan.md"
    src = tmp_path / "plan_src.md"
    src.write_text(VALID_PLAN, encoding="utf-8")
    # 2: write agent settles WITHOUT writing the file → corrective retry;
    # 3: second attempt writes it; 4: todo regen.
    fake_pi.set_script(
        ["sentinel:READY_TO_PLAN", "turns:1", f"copy:{src}>{plan_path}", "ok"]
    )

    stage.start(task)
    worker._dispatch(queue.claim_next())
    worker._dispatch(queue.claim_next())

    state = paths.plan_state(task).read()
    assert state["phase"] == "done", state.get("error")
    corrective = fake_pi.prompt(3)
    assert "Verification failed" in corrective
    assert "does not exist" in corrective
    assert str(plan_path) in corrective


def test_critique_no_questions_chains_to_verified_revise(task, fake_pi):
    review = review_stage()
    paths.write_plan_artefact(task, "final_plan.md", VALID_PLAN)
    # 1: critic has nothing to ask; 2: auto-revise settles without editing
    # (legal on the auto path — the plan may need no changes); 3: todo regen.
    fake_pi.set_script(["sentinel:READY_TO_REVISE", "turns:1", "ok"])

    review.start(task)
    job = queue.claim_next()
    assert job is not None and (job["slug"], job["step"]) == ("plan_review", "critique")
    worker._dispatch(job)

    revise_job = queue.claim_next()
    assert revise_job is not None
    assert (revise_job["slug"], revise_job["step"]) == ("plan_review", "revise")
    assert paths.plan_review_state(task).read()["phase"] == "revising"

    worker._dispatch(revise_job)
    state = paths.plan_review_state(task).read()
    assert state["phase"] == "awaiting_approval", state.get("error")
    assert state["iterations"] == 1
    assert not state.get("rounds"), "no synthetic question round is fabricated"


# ---------------------------------------------------------------------------
# run_until_verified — settle → verify → corrective-retry driver
# ---------------------------------------------------------------------------


def _driver(run_hop, verify, **kw):
    return run_until_verified(
        start=time.monotonic(),
        timeout_seconds=60,
        message="go",
        run_hop=run_hop,
        verify=verify,
        corrective=lambda d: f"fix: {d}",
        on_stopped=kw.pop("on_stopped", lambda: None),
        **kw,
    )


def test_run_until_verified_passes_first_try():
    calls: list[str] = []

    def run_hop(msg, hop):
        calls.append(msg)
        return "agent_end"

    assert _driver(run_hop, lambda: None) == "verified"
    assert calls == ["go"]


def test_run_until_verified_corrective_then_pass():
    calls: list[str] = []
    deficiencies = ["file missing", None]

    def run_hop(msg, hop):
        calls.append(msg)
        return "agent_end"

    assert _driver(run_hop, lambda: deficiencies.pop(0)) == "verified"
    assert calls == ["go", "fix: file missing"]


def test_run_until_verified_exhausts_correctives():
    calls: list[str] = []

    def run_hop(msg, hop):
        calls.append(msg)
        return "agent_end"

    with pytest.raises(RuntimeError, match="corrective"):
        _driver(run_hop, lambda: "still broken", max_corrective=2)
    assert len(calls) == 3  # initial + 2 correctives


def test_run_until_verified_time_cap_still_verifies():
    assert _driver(lambda m, h: "time_cap", lambda: None) == "verified"
    with pytest.raises(RuntimeError, match="time cap"):
        _driver(lambda m, h: "time_cap", lambda: "file missing")


def test_run_until_verified_stopped_runs_callback():
    stopped: list[bool] = []
    out = _driver(
        lambda m, h: "stopped", lambda: None, on_stopped=lambda: stopped.append(True)
    )
    assert out == "stopped" and stopped == [True]


# ---------------------------------------------------------------------------
# verify_artifact_file — exists + fresh + headings
# ---------------------------------------------------------------------------


def test_verify_artifact_file_checks(tmp_path):
    p = tmp_path / "plan.md"
    assert "does not exist" in verify_artifact_file(p, None)

    p.write_text("# Plan\n\n## Changes\nstuff\n", encoding="utf-8")
    assert verify_artifact_file(p, None, ("# Plan", "## Changes")) is None

    missing = verify_artifact_file(p, None, ("# Plan", "## Automatic verification"))
    assert "missing required section" in missing
    assert "## Automatic verification" in missing

    unchanged = file_content_hash(p)
    assert "not modified" in verify_artifact_file(p, unchanged)
    assert verify_artifact_file(p, unchanged, require_change=False) is None

    p.write_text("# Plan\n\n## Changes\nnew stuff\n", encoding="utf-8")
    assert verify_artifact_file(p, unchanged, ("# Plan",)) is None


def test_verify_artifact_file_heading_prefix_match(tmp_path):
    p = tmp_path / "plan.md"
    p.write_text("## Overview / Summary\nrecap\n", encoding="utf-8")
    assert verify_artifact_file(p, None, ("## Overview",)) is None
    # A heading buried mid-line does not count.
    p.write_text("text mentioning ## Overview inline\n", encoding="utf-8")
    assert "missing required section" in verify_artifact_file(p, None, ("## Overview",))


# ---------------------------------------------------------------------------
# RC6 — orphan-phase watchdog
# ---------------------------------------------------------------------------


def _age_state_file(path: Path, seconds: float = 60.0) -> None:
    old = time.time() - seconds
    os.utime(path, (old, old))


def test_check_orphaned_flags_running_phase_without_job(task, fake_pi):
    stage = plan_stage()
    state = paths.plan_state(task)
    state.write({"phase": "write_running"})

    # Fresh state file: the grace window protects the enqueue gap.
    assert stage.check_orphaned(task) is False
    assert state.read()["phase"] == "write_running"

    _age_state_file(state.path)
    assert stage.check_orphaned(task) is True
    after = state.read()
    assert after["phase"] == "error"
    assert "no queued job" in after["error"]


def test_check_orphaned_leaves_backed_and_settled_phases_alone(task, fake_pi):
    stage = plan_stage()
    state = paths.plan_state(task)

    # Running phase WITH a job behind it: never flagged, however old.
    state.write({"phase": "write_running"})
    _age_state_file(state.path)
    queue.enqueue(task, "plan", "write")
    assert stage.check_orphaned(task) is False
    assert state.read()["phase"] == "write_running"
    queue.complete(queue.claim_next())

    # Non-running phases are never flagged.
    for phase in ("awaiting_answers", "done", "stopped", "idle"):
        state.write({"phase": phase})
        _age_state_file(state.path)
        assert stage.check_orphaned(task) is False
        assert state.read()["phase"] == phase
