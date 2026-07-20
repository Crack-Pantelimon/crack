"""Plan 4.1 tests: runner & stage lifecycle, driven by the fake_pi.sh shim.

Covers the per-section acceptance checks that are practical as pytest cases:
rate limiting (§2), transient retries + session resume (§3), cap removal (§4),
STOP classification (§6 backend), the exclusive queue (§7), sentinel own-line
matching (§8), and compiled-prompt recording (§1) — including one stage-level
run of Explore against the shim.

Retry-everything update: hard failures (SIGKILL/-9 included) and empty runs
retry on the HARD_RETRY_DELAYS schedule; the no-progress streak cap is
len(HARD_RETRY_DELAYS) + 1 attempts, and the durable error budget
(MAX_TOTAL_ERRORS recorded errors) raises PiError(over_budget=True).
"""

from __future__ import annotations

import asyncio
import os
import shutil
import threading
import time
from pathlib import Path

import pytest

from crack_server import pi_proc, pi_runner, queue, ratelimit

SHIM = Path(__file__).parent / "fake_pi.sh"


class FakePi:
    def __init__(self, ctrl: Path, script: Path):
        self.ctrl = ctrl
        self.script = script

    def set_script(self, lines: list[str]) -> None:
        self.script.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def argv(self, n: int) -> list[str]:
        return (self.ctrl / f"argv.{n}").read_text(encoding="utf-8").splitlines()

    def prompt(self, n: int) -> str:
        return (self.ctrl / f"prompt.{n}").read_text(encoding="utf-8")

    def invocations(self) -> int:
        count = self.ctrl / "count"
        return int(count.read_text()) if count.exists() else 0


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
    # Fast retry schedules so failure paths finish in well under a second each.
    monkeypatch.setattr(ratelimit, "TRANSIENT_RETRY_DELAYS", [0.05, 0.05, 0.05])
    monkeypatch.setattr(ratelimit, "HARD_RETRY_DELAYS", [0.05, 0.05, 0.05, 0.05])
    monkeypatch.setattr(ratelimit, "PI_RETRY_WINDOW_SECONDS", 0.2)
    return FakePi(ctrl, script)


def run_hop(tmp_path, message="do it", sentinel=None, model="moonshotai/x", **kw):
    turns: list[dict] = []
    reason = pi_runner.run_agent_hop(
        log_prefix="test",
        model=model,
        session_id="hop-test",
        sessions_dir=tmp_path / "sessions",
        tools="bash",
        message=message,
        start=time.monotonic(),
        sentinel=sentinel,
        timeout_seconds=60,
        persist_turn=lambda t, h: turns.append(dict(t)),
        **kw,
    )
    return reason, turns


# ---------------------------------------------------------------------------
# §2 — provider-keyed rate limiter, lock-free waits
# ---------------------------------------------------------------------------


def test_limiter_keyed_by_provider():
    assert pi_runner.limiter_for("moonshotai/kimi-k2.6") is None
    assert pi_runner.limiter_for("z-ai/glm-5.2") is None
    a = pi_runner.limiter_for("nvidia/nemotron-3-nano-30b-a3b")
    b = pi_runner.limiter_for("nvidia/something-else")
    assert a is not None and a is b


def test_rate_limiter_reserves_slots_without_serializing():
    rl = pi_runner.RateLimiter("test", calls_per_minute=600)  # 0.1s spacing
    rl.wait()  # claim slot 0 immediately
    t0 = time.monotonic()
    done: list[float] = []

    def waiter():
        rl.wait()
        done.append(time.monotonic() - t0)

    threads = [threading.Thread(target=waiter) for _ in range(2)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    # Both waiters slept concurrently on pre-reserved slots (0.1s and 0.2s):
    # the later one finishes ~0.2s in, not 0.1s + 0.1s serialized behind a lock
    # plus contention. With the old lock-held-across-sleep design the asserts
    # still hold for n=2, so mainly check the spacing schedule is respected.
    assert min(done) >= 0.08
    assert max(done) == pytest.approx(0.2, abs=0.12)


def test_non_nvidia_hops_run_back_to_back(fake_pi, tmp_path):
    fake_pi.set_script(["turns:1", "turns:1"])
    t0 = time.monotonic()
    run_hop(tmp_path, model="moonshotai/x")
    run_hop(tmp_path, model="moonshotai/x")
    # No limiter engaged: the spacing is only subprocess startup cost.
    assert time.monotonic() - t0 < 2.0


# ---------------------------------------------------------------------------
# §3 — transient classification, retries, session resume
# ---------------------------------------------------------------------------


def test_is_transient_classification():
    assert pi_runner.is_transient("google.api_core.ResourceExhausted: 429")
    assert pi_runner.is_transient("HTTP 429 Too Many Requests")
    assert pi_runner.is_transient("model is overloaded, temporarily unavailable")
    assert pi_runner.is_transient("connection reset by peer")
    assert pi_runner.is_transient("upstream 503")
    assert not pi_runner.is_transient("SyntaxError: invalid syntax")
    assert not pi_runner.is_transient("boom: unrecoverable parse explosion")
    assert not pi_runner.is_transient("")


def test_transient_then_success_completes_one_trajectory(fake_pi, tmp_path):
    fake_pi.set_script(["transient", "transient", "turns:2"])
    reason, turns = run_hop(tmp_path)
    assert reason == "agent_end"
    assert len(turns) == 2
    assert fake_pi.invocations() == 3
    # No turns persisted before the failures → the original message is replayed.
    assert fake_pi.prompt(3) == "do it"


def test_midstream_kill_resumes_session_with_continuation(fake_pi, tmp_path):
    # 2 turns stream, then a transient death; the reattempt must resume the
    # same session with the continuation message, keeping turns 1-2.
    fake_pi.set_script(["midfail:2", "turns:1"])
    prompts: list[dict] = []
    reason, turns = run_hop(tmp_path, record_prompt=prompts.append)
    assert reason == "agent_end"
    assert len(turns) == 3
    assert fake_pi.prompt(2) == pi_runner.RESUME_MESSAGE
    a1, a2 = fake_pi.argv(1), fake_pi.argv(2)
    assert a1[a1.index("--session-id") + 1] == a2[a2.index("--session-id") + 1]
    assert a1[a1.index("--session-dir") + 1] == a2[a2.index("--session-dir") + 1]
    # §1: the prompt was recorded once per hop call (not per attempt).
    assert len(prompts) == 1
    assert prompts[0]["kind"] == "user_prompt"
    assert prompts[0]["compiled"] == "do it"
    assert prompts[0]["hop"] == 1


def test_transient_failures_raise_at_streak_cap(fake_pi, tmp_path):
    fake_pi.set_script(["transient"])
    errors: list[dict] = []
    with pytest.raises(pi_runner.PiError) as excinfo:
        run_hop(tmp_path, record_error=lambda e: errors.append(e) or len(errors))
    # Transients also retry until the no-progress streak cap: every attempt
    # failed without persisting a turn, so the streak ends it (not the budget).
    assert fake_pi.invocations() == 1 + len(ratelimit.HARD_RETRY_DELAYS)
    assert not excinfo.value.over_budget
    assert len(errors) == 1 + len(ratelimit.HARD_RETRY_DELAYS)


def test_hard_failure_after_persisted_turns_resumes_and_retries(fake_pi, tmp_path):
    # A hard failure after persisted turns no longer raises immediately: the
    # hop resumes the session (RESUME_MESSAGE) and retries on the hard
    # schedule. Here every attempt makes progress, so only the error budget
    # (MAX_TOTAL_ERRORS recorded errors) stops it, over_budget=True.
    fake_pi.set_script(["midhard:2"])
    persisted: list[dict] = []
    errors: list[dict] = []
    with pytest.raises(pi_runner.PiError) as excinfo:
        pi_runner.run_agent_hop(
            log_prefix="test",
            model="moonshotai/x",
            session_id="hop-test",
            sessions_dir=tmp_path / "sessions",
            tools="bash",
            message="do it",
            start=time.monotonic(),
            sentinel=None,
            timeout_seconds=60,
            persist_turn=lambda t, h: persisted.append(dict(t)),
            record_error=lambda e: errors.append(e) or len(errors),
        )
    assert excinfo.value.over_budget is True
    # Every failed attempt was recorded durably with the entry fields.
    assert len(errors) == ratelimit.MAX_TOTAL_ERRORS
    assert errors[0]["rc"] == 1 and errors[0]["attempt"] == 1
    assert errors[0]["message"] == "pi exited 1"
    # Turns 1-2 of every attempt were kept and the session was resumed.
    assert len(persisted) == 2 * ratelimit.MAX_TOTAL_ERRORS
    assert fake_pi.invocations() == ratelimit.MAX_TOTAL_ERRORS
    assert fake_pi.prompt(2) == pi_runner.RESUME_MESSAGE


def test_error_budget_cap_raises_over_budget(fake_pi, tmp_path):
    fake_pi.set_script(["hard"])
    errors: list[dict] = []
    with pytest.raises(pi_runner.PiError) as excinfo:
        run_hop(
            tmp_path,
            record_error=lambda e: errors.append(e) or len(errors),
            error_budget=lambda: 3,
        )
    assert excinfo.value.over_budget is True
    assert fake_pi.invocations() == 3
    assert len(errors) == 3


def test_broken_error_recorder_never_wedges_retries(fake_pi, tmp_path):
    fake_pi.set_script(["hard"])

    def broken(entry: dict) -> int:
        raise RuntimeError("recorder exploded")

    with pytest.raises(pi_runner.PiError):
        run_hop(tmp_path, record_error=broken)
    # The streak cap still stops the loop on the local fallback count.
    assert fake_pi.invocations() == 1 + len(ratelimit.HARD_RETRY_DELAYS)


# ---------------------------------------------------------------------------
# Terminal-aware exit grace — no phantom SIGKILL after agent_end
# ---------------------------------------------------------------------------


def test_terminal_linger_past_grace_is_not_sigkill(fake_pi, tmp_path, monkeypatch):
    # Fake pi emits agent_end then sleeps longer than EXIT_GRACE_SECONDS.
    # The harness must detach (not SIGKILL) and must not record "pi exited -9".
    monkeypatch.setattr(pi_proc, "EXIT_GRACE_SECONDS", 0.4)
    fake_pi.set_script(["linger:2"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert len(turns) == 1
    assert turns[0]["text"] == "done, lingering (invocation 1)"
    assert fake_pi.invocations() == 1
    assert errors == []
    assert not any("pi exited -9" in (e.get("message") or "") for e in errors)


def test_nonzero_exit_without_terminal_still_retries(fake_pi, tmp_path):
    # Guards the not-sink.terminal branch: a hard crash with no agent_end must
    # still count as failed and be retried (not short-circuited by terminality).
    fake_pi.set_script(["hard", "turns:1"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert len(turns) == 1
    assert fake_pi.invocations() == 2
    assert len(errors) == 1
    assert errors[0]["message"] == "pi exited 1"
    assert errors[0]["rc"] == 1


def test_auto_retry_end_after_progress_forces_resume(fake_pi, tmp_path):
    # Progress then pi-internal auto_retry exhaustion (rc 0 + agent_settled)
    # must not look "done": the hop retries with RESUME_MESSAGE and keeps the
    # earlier turns. Second attempt finishing cleanly completes the exchange.
    fake_pi.set_script(["autoretryfail:1", "turns:1"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert len(turns) == 2
    assert turns[0]["text"] == "turn 1 (invocation 1)"
    assert turns[1]["text"] == "turn 1 (invocation 2)"
    assert fake_pi.invocations() == 2
    assert fake_pi.prompt(2) == pi_runner.RESUME_MESSAGE
    assert len(errors) == 1
    assert errors[0]["message"] == "pi gave up: 429 status code (no body)"
    assert errors[0]["rc"] == 0
    # Detail must surface the attempt's stdout/stderr tail (not hardcoded "").
    assert errors[0]["detail"]
    assert "auto_retry_end" in errors[0]["detail"] or "429" in errors[0]["detail"]


def test_empty_turns_error_includes_detail(fake_pi, tmp_path):
    # Clean agent_end with no persisted turns retries like a hard failure; the
    # recorded error must carry the hop output tail for the UI collapsible.
    fake_pi.set_script(["turns:0", "turns:1"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert len(turns) == 1
    assert fake_pi.invocations() == 2
    assert len(errors) == 1
    assert errors[0]["message"] == "pi returned only empty turns"
    assert errors[0]["detail"]
    assert "agent_end" in errors[0]["detail"]


def test_persisted_then_clean_agent_end_still_returns_immediately(fake_pi, tmp_path):
    # Regression: a real persisted turn followed by a clean agent_end (no
    # auto_retry_end failure) must still short-circuit as done — no over-retry.
    fake_pi.set_script(["turns:1"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert len(turns) == 1
    assert fake_pi.invocations() == 1
    assert errors == []


def test_willretry_agent_end_does_not_end_hop_early(fake_pi, tmp_path):
    # Regression: a single pi invocation can emit several internal agent_end
    # events (willRetry=true) before the CLI process actually exits. Only a
    # willRetry=false agent_end (or agent_settled) should end the hop — an
    # earlier willRetry=true agent_end must not be mistaken for process exit,
    # which used to truncate output and lose every later turn.
    fake_pi.set_script(["willretry:2:3"])
    errors: list[dict] = []
    reason, turns = run_hop(
        tmp_path,
        record_error=lambda e: errors.append(e) or len(errors),
    )
    assert reason == "agent_end"
    assert [t["text"] for t in turns] == [
        "phase1 turn 1 (invocation 1)",
        "phase1 turn 2 (invocation 1)",
        "phase2 turn 1 (invocation 1)",
        "phase2 turn 2 (invocation 1)",
        "phase2 turn 3 (invocation 1)",
    ]
    # All turns come from one pi process — no retry/resume was triggered.
    assert fake_pi.invocations() == 1
    assert errors == []


def test_hard_backoff_schedule_matches_hard_retry_delays(monkeypatch):
    sleeps: list[float] = []

    async def fake_sleep(delay: float) -> None:
        sleeps.append(delay)

    monkeypatch.setattr(asyncio, "sleep", fake_sleep)
    # Cover a progress-reset streak of 0, every indexed delay, and one past the
    # end (clamps at the last HARD_RETRY_DELAYS entry).
    streaks = list(range(0, len(ratelimit.HARD_RETRY_DELAYS) + 2))
    for streak in streaks:
        asyncio.run(ratelimit._async_hard_backoff_sleep(streak))
    # streak 0 → index 0 (1s); streak k → HARD_RETRY_DELAYS[k-1]; past end clamps.
    expected = [
        ratelimit.HARD_RETRY_DELAYS[
            max(0, min(s - 1, len(ratelimit.HARD_RETRY_DELAYS) - 1))
        ]
        for s in streaks
    ]
    assert sleeps == expected
    assert sleeps == [1.0, 1.0, 3.0, 6.0, 9.0, 16.0, 27.0, 27.0]


def test_no_progress_streak_resets_on_progress(fake_pi, tmp_path, monkeypatch):
    streaks: list[int] = []

    async def fake_hard_sleep(streak: int) -> None:
        streaks.append(streak)

    monkeypatch.setattr(pi_proc, "_async_hard_backoff_sleep", fake_hard_sleep)
    fake_pi.set_script(["hard", "hard", "midhard:1", "hard"])
    errors: list[dict] = []
    persisted: list[dict] = []
    with pytest.raises(pi_runner.PiError) as excinfo:
        pi_runner.run_agent_hop(
            log_prefix="test",
            model="moonshotai/x",
            session_id="hop-test",
            sessions_dir=tmp_path / "sessions",
            tools="bash",
            message="do it",
            start=time.monotonic(),
            sentinel=None,
            timeout_seconds=60,
            persist_turn=lambda t, h: persisted.append(dict(t)),
            record_error=lambda e: errors.append(e) or len(errors),
        )
    # Streak climbs 1,2 → midhard persists a turn (progress → 0, resume) →
    # climbs again until the cap (len(HARD_RETRY_DELAYS) + 1 attempts).
    assert streaks == [1, 2, 0, 1, 2, 3, 4]
    assert fake_pi.invocations() == 8
    assert len(persisted) == 1
    assert len(errors) == 8
    assert not excinfo.value.over_budget
    # After the progressing failure the session was resumed, not replayed.
    assert fake_pi.prompt(4) == pi_runner.RESUME_MESSAGE


def test_run_pi_text_transient_then_ok(fake_pi):
    fake_pi.set_script(["transient", "transient", "ok"])
    text, _ = pi_runner.run_pi_text("hello", log_prefix="t", model="moonshotai/x")
    assert text == "text-response"
    assert fake_pi.invocations() == 3


def test_run_pi_text_hard_failures_exhaust_schedule(fake_pi):
    fake_pi.set_script(["hard"])
    with pytest.raises(pi_runner.PiError):
        pi_runner.run_pi_text("hello", log_prefix="t", model="moonshotai/x")
    assert fake_pi.invocations() == pi_runner.PI_RETRY_ATTEMPTS


def test_run_pi_text_records_each_failed_attempt(fake_pi):
    fake_pi.set_script(["hard"])
    errors: list[dict] = []
    with pytest.raises(pi_runner.PiError):
        pi_runner.run_pi_text(
            "hello",
            log_prefix="t",
            model="moonshotai/x",
            record_error=lambda e: errors.append(e) or len(errors),
        )
    # One durable error row per failed attempt (PI_RETRY_ATTEMPTS total).
    assert len(errors) == pi_runner.PI_RETRY_ATTEMPTS
    assert [e["attempt"] for e in errors] == list(
        range(1, pi_runner.PI_RETRY_ATTEMPTS + 1)
    )
    assert all(e["message"] == "pi exited 1" for e in errors)
    assert all(e["phase"] == "t" for e in errors)


# ---------------------------------------------------------------------------
# §4 — no turn caps
# ---------------------------------------------------------------------------


def test_forty_turns_stream_uncut(fake_pi, tmp_path):
    fake_pi.set_script(["turns:40"])
    reason, turns = run_hop(tmp_path)
    assert reason == "agent_end"
    assert len(turns) == 40


# ---------------------------------------------------------------------------
# §8 — sentinel matches only on its own line
# ---------------------------------------------------------------------------


def test_sentinel_own_line_only(fake_pi, tmp_path):
    fake_pi.set_script(["inline:STOPWORD", "sentinel:STOPWORD"])
    reason, _ = run_hop(tmp_path, sentinel="STOPWORD")
    assert reason == "agent_end"  # mid-line mention does not stop the hop
    reason, _ = run_hop(tmp_path, sentinel="STOPWORD")
    assert reason == "sentinel"


# ---------------------------------------------------------------------------
# §6 backend — STOP kills the process group and classifies cleanly
# ---------------------------------------------------------------------------


def test_stop_kills_process_group_and_returns_stopped(fake_pi, tmp_path):
    fake_pi.set_script(["sleepy:30"])
    stop_flag = {"v": False}
    pid_file = tmp_path / "agent.pid"
    result: dict = {}

    def target():
        try:
            reason, turns = run_hop(
                tmp_path, pid_file=pid_file, stop_check=lambda: stop_flag["v"]
            )
            result["reason"] = reason
            result["turns"] = turns
        except Exception as e:  # pragma: no cover - failure surface
            result["exc"] = e

    thread = threading.Thread(target=target)
    thread.start()
    deadline = time.monotonic() + 10
    while not pid_file.exists() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert pid_file.exists(), "pid file was never published"
    time.sleep(0.5)  # let the first turn stream before killing

    stop_flag["v"] = True
    t_kill = time.monotonic()
    assert pi_runner.kill_pid_file(pid_file)
    thread.join(timeout=10)
    assert not thread.is_alive()
    assert time.monotonic() - t_kill < 5.0
    assert result.get("reason") == "stopped"
    assert len(result.get("turns", [])) == 1  # the pre-sleep turn survived


# ---------------------------------------------------------------------------
# §7 — exclusive enqueue
# ---------------------------------------------------------------------------


def test_enqueue_exclusive_drops_duplicates(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    first = queue.enqueue_exclusive("t1", "explore", "run")
    assert first is not None
    assert queue.enqueue_exclusive("t1", "explore", "resume") is None
    assert queue.enqueue_exclusive("t1", "plan", "draft") is not None
    assert queue.enqueue_exclusive("t2", "explore", "run") is not None

    job = queue.claim_next()
    assert job is not None
    # Still exclusive while the claimed job sits in processing/.
    assert queue.enqueue_exclusive(job["task_id"], job["slug"], job["step"]) is None
    # …unless the caller exempts its own in-flight job (RC1: a running step
    # enqueueing its stage's successor must not collide with itself).
    chained = queue.enqueue_exclusive(
        job["task_id"], job["slug"], "next", ignore_job_id=job["id"]
    )
    assert chained is not None
    # The chained job now guards the slug again for everyone else.
    assert queue.enqueue_exclusive(job["task_id"], job["slug"], "next") is None
    queue.complete(job)
    drained = []
    while (j := queue.claim_next()) is not None:
        drained.append(j)
        queue.complete(j)
    assert any(
        (j["task_id"], j["slug"], j["step"]) == (job["task_id"], job["slug"], "next")
        for j in drained
    )
    assert queue.enqueue_exclusive(job["task_id"], job["slug"], job["step"]) is not None


# ---------------------------------------------------------------------------
# §1 — prompt entries: skipped by counters/renderers
# ---------------------------------------------------------------------------


def test_prompt_entries_skipped_by_turn_helpers():
    turns = [
        {"kind": "user_prompt", "compiled": "PROMPTBLOB", "at": 0.0},
        {"text": "a", "thinking": "", "tool_blocks": []},
        {"kind": "user_prompt", "compiled": "PROMPTBLOB2", "at": 1.0},
        {"text": "", "thinking": "", "tool_blocks": [{"name": "bash", "input": "ls"}]},
    ]
    assert pi_runner.count_turn_groups(turns) == 2
    transcript = pi_runner.render_transcript_plaintext(turns)
    assert "PROMPTBLOB" not in transcript
    assert "--- Turn 2" in transcript
