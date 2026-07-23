"""Per-model EMA latency: clamp, first-value, file tolerance, concurrent writes."""

from __future__ import annotations

import asyncio
import json

import pytest

from crack_server import model_latency, paths


@pytest.fixture
def latency_root(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    return tmp_path


@pytest.mark.anyio
async def test_record_latency_first_value_is_clamped(latency_root):
    await model_latency.record_latency("m1", 12.5)
    assert model_latency.latencies()["m1"] == pytest.approx(12.5)


@pytest.mark.anyio
async def test_record_latency_ema_and_clamp(latency_root):
    await model_latency.record_latency("m1", 10.0)
    await model_latency.record_latency("m1", 20.0)
    # new = 10*0.9 + 20*0.1 = 11.0
    assert model_latency.latencies()["m1"] == pytest.approx(11.0)

    await model_latency.record_latency("m2", 0.01)  # clamp up to 0.1
    assert model_latency.latencies()["m2"] == pytest.approx(0.1)

    await model_latency.record_latency("m3", 999.0)  # clamp down to 400
    assert model_latency.latencies()["m3"] == pytest.approx(400.0)


def test_latencies_tolerates_missing_and_corrupt(latency_root):
    assert model_latency.latencies() == {}
    path = paths.model_latency_state().path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("not-json{{{", encoding="utf-8")
    assert model_latency.latencies() == {}


@pytest.mark.anyio
async def test_concurrent_record_latency_does_not_corrupt(latency_root):
    async def _one(i: int) -> None:
        await model_latency.record_latency("shared", 10.0 + (i % 5))

    await asyncio.gather(*(_one(i) for i in range(20)))
    data = paths.model_latency_state().read()
    assert "shared" in data
    # File must still be valid JSON with a finite float.
    val = float(data["shared"])
    assert 0.1 <= val <= 400.0
    # Round-trip through json.dumps to ensure no corruption residue.
    json.loads(paths.model_latency_state().path.read_text(encoding="utf-8"))


@pytest.mark.anyio
async def test_flush_latencies_no_double_count_across_per_hop_persisters(
    latency_root, tmp_path
):
    """The sub-agent path builds a fresh TurnPersister each hop (existing =
    all prior turns). Re-flushing must not re-record earlier turns' deltas."""
    from crack_server.state import JsonState
    from crack_server.steprun import TurnPersister, flush_latencies

    state = JsonState(tmp_path / "run.json")
    state.write({"turns": []})
    ats = [0.0, 10.0, 40.0, 60.0]  # deltas 10, 30, 20

    for i, at in enumerate(ats):
        # New persister per hop, exactly like sub_agents.base._run_hop.
        p = TurnPersister(state, key="turns")
        p.append({"model": "M", "at": at, "text": f"t{i}", "tool_blocks": []})
        await flush_latencies(p)

    # Correct running EMA over 10 -> 30 -> 20:
    #   10, then 10*.9+30*.1=12, then 12*.9+20*.1=12.8
    assert model_latency.latencies()["M"] == pytest.approx(12.8)


@pytest.mark.anyio
async def test_flush_latencies_ignores_compiled_prompt_entries(latency_root, tmp_path):
    """Compiled-prompt entries (carry ``template``) sit in the turns list but
    must not be treated as agent turns when computing deltas."""
    from crack_server.state import JsonState
    from crack_server.steprun import TurnPersister, flush_latencies

    state = JsonState(tmp_path / "run.json")
    state.write({"turns": []})

    p = TurnPersister(state, key="turns")
    p.append({"template": "chat.md", "at": 0.0})  # compiled prompt, not a turn
    p.append({"model": "M", "at": 5.0, "text": "a", "tool_blocks": []})
    p.append({"model": "M", "at": 15.0, "text": "b", "tool_blocks": []})
    await flush_latencies(p)

    # Only the two real turns count -> one delta of 10s recorded (first sample).
    assert model_latency.latencies()["M"] == pytest.approx(10.0)
