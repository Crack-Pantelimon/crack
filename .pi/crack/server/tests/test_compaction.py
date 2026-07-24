"""Tests for rolling summarizer compaction."""

from __future__ import annotations

import asyncio
import json
import time
from pathlib import Path

import pytest

from crack_server import compaction, models, render as render_mod
from crack_server.state import JsonState


@pytest.fixture()
def _no_models_cache(monkeypatch):
    monkeypatch.setattr(models, "model_info", lambda _m: None)


def _assistant_tool_call(call_id: str = "tc1") -> dict:
    return {
        "type": "message",
        "id": f"a-{call_id}",
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "toolCall",
                    "id": call_id,
                    "name": "read",
                    "arguments": {"path": "x"},
                }
            ],
        },
    }


def _tool_result(call_id: str = "tc1", text: str = "ok") -> dict:
    return {
        "type": "message",
        "id": f"tr-{call_id}",
        "message": {
            "role": "toolResult",
            "toolCallId": call_id,
            "toolName": "read",
            "content": [{"type": "text", "text": text}],
        },
    }


def _user_msg(text: str, mid: str = "u1") -> dict:
    return {
        "type": "message",
        "id": mid,
        "message": {
            "role": "user",
            "content": [{"type": "text", "text": text}],
        },
    }


def _big_events(n: int, chunk: str = "x" * 400) -> list[dict]:
    return [_user_msg(f"{chunk}-{i}", mid=f"u{i}") for i in range(n)]


class TestShouldCompact:
    def test_below_threshold(self, tmp_path, monkeypatch, _no_models_cache):
        monkeypatch.setattr(
            compaction, "session_usage", lambda _d: {"tokens": 149_999, "output": 1}
        )
        assert compaction.should_compact(tmp_path, "unknown/model") is False

    def test_at_threshold(self, tmp_path, monkeypatch, _no_models_cache):
        monkeypatch.setattr(
            compaction, "session_usage", lambda _d: {"tokens": 150_000, "output": 1}
        )
        assert compaction.should_compact(tmp_path, "unknown/model") is True


class TestFindCutoff:
    def test_preserves_tool_group(self):
        events = [
            _user_msg("old"),
            _assistant_tool_call("tc1"),
            _tool_result("tc1"),
            _user_msg("recent " + "y" * 2000, mid="u2"),
        ]
        cut = compaction._find_cutoff_index(events, retain_tokens=500)
        retained = events[cut:]
        roles = [
            e.get("message", {}).get("role")
            for e in retained
            if e.get("type") == "message"
        ]
        assert "toolResult" not in roles or "assistant" in roles
        assert roles[0] != "toolResult"

    def test_retain_tail_token_budget(self):
        events = _big_events(80)
        cut = compaction._find_cutoff_index(events, retain_tokens=20_000)
        retained = events[cut:]
        est = sum(compaction._estimate_event_tokens(e) for e in retained)
        assert est >= 20_000 or cut == 0


class TestFallbackSummary:
    def test_has_headings(self):
        events = [_user_msg("build feature X")]
        transcript = compaction._events_transcript(events)
        summary = compaction._fallback_summary(transcript, events)
        for heading in ("# Goal", "# Progress", "# Key decisions", "# Current state", "# Open items"):
            assert heading in summary


class TestSeedCompactedSession:
    def test_creates_valid_jsonl(self, tmp_path):
        retained = [_user_msg("tail")]
        path = compaction.seed_compacted_session(
            tmp_path,
            "chat-c1",
            summary="# Goal\n- test",
            retained_events=retained,
        )
        assert path.is_file()
        lines = path.read_text(encoding="utf-8").strip().splitlines()
        assert len(lines) == 3
        first = json.loads(lines[0])
        second = json.loads(lines[1])
        third = json.loads(lines[2])
        assert first["type"] == "session"
        assert first["id"] == "chat-c1"
        assert second["message"]["role"] == "user"
        from crack_server.transcript import text_from_content
        assert "compaction summary" in text_from_content(second["message"]["content"]).lower()
        assert third["message"]["content"][0]["text"] == "tail"


class TestCompactIfNeeded:
    def test_updates_state_and_records_note(self, tmp_path, monkeypatch, _no_models_cache):
        sessions = tmp_path / "sessions"
        sessions.mkdir()
        events = _big_events(60) + [_user_msg("keep me", mid="tail")]
        (sessions / "sess.jsonl").write_text(
            "\n".join(json.dumps(e) for e in events) + "\n",
            encoding="utf-8",
        )
        state_path = tmp_path / "state.json"
        state_obj = JsonState(state_path)
        state_obj.write({"traj_notes": []})

        monkeypatch.setattr(compaction, "should_compact", lambda _d, _m: True)
        monkeypatch.setattr(compaction, "session_usage", lambda _d: {"tokens": 160_000, "output": 1})

        async def _fake_summary(transcript, model):
            return "# Goal\n- summarized\n\n# Progress\n- ok\n\n# Key decisions\n-\n\n# Current state\n-\n\n# Open items\n-", "llm"

        monkeypatch.setattr(compaction, "generate_summary", _fake_summary)

        active = asyncio.run(compaction.compact_if_needed(
            state_obj=state_obj,
            sessions_dir=sessions,
            model="unknown/model",
            base_session_id="chat-1",
            pid_file=None,
            log_prefix="test",
        ))
        assert active == "chat-1-c1"
        s = state_obj.read()
        assert s["pi_session_id"] == "chat-1-c1"
        assert s["compaction_count"] == 1
        notes = s.get("traj_notes") or []
        assert notes[-1]["note_type"] == "compaction"
        assert notes[-1]["status"] == "ok"
        assert notes[-1]["tokens_before"] == 160_000

    def test_failure_records_err_note(self, tmp_path, monkeypatch, _no_models_cache):
        sessions = tmp_path / "sessions"
        sessions.mkdir()
        (sessions / "sess.jsonl").write_text(
            "\n".join(json.dumps(e) for e in _big_events(8)) + "\n",
            encoding="utf-8",
        )
        state_obj = JsonState(tmp_path / "state.json")
        state_obj.write({})

        monkeypatch.setattr(compaction, "should_compact", lambda _d, _m: True)
        monkeypatch.setattr(compaction, "session_usage", lambda _d: {"tokens": 200_000, "output": 1})

        def _boom(*_a, **_k):
            raise RuntimeError("seed failed")

        monkeypatch.setattr(compaction, "seed_compacted_session", _boom)

        active = asyncio.run(compaction.compact_if_needed(
            state_obj=state_obj,
            sessions_dir=sessions,
            model="unknown/model",
            base_session_id="chat-1",
            pid_file=None,
            log_prefix="test",
        ))
        assert active == "chat-1"
        note = (state_obj.read().get("traj_notes") or [])[-1]
        assert note["status"] == "err"


def test_render_compaction_note_html():
    html = render_mod.render_note_row({
        "kind": "note",
        "note_type": "compaction",
        "status": "ok",
        "text": "Context compacted (llm)",
        "tokens_before": 150_000,
        "tokens_after": 18_000,
        "messages_before": 40,
        "messages_after": 12,
        "duration_s": 2.5,
        "at": time.time(),
    })
    assert "traj-note--compaction" in html
    assert "150,000" in html
    assert "18,000" in html
    assert "msgs 40→12" in html
    assert "2.5s" in html
