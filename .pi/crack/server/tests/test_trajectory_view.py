"""Plan 24 Issue 2: trajectory projection from pi session ndjson."""

from __future__ import annotations

import json
from pathlib import Path

from crack_server import git_utils, trajectory_view


def test_project_unknown_event_has_expand_row(tmp_path: Path):
    trajectory_view.clear_cache()
    sessions = tmp_path / "sessions"
    sessions.mkdir()
    (sessions / "2026-01-01T00-00-00Z_s.jsonl").write_text(
        "\n".join([
            json.dumps({"type": "session", "id": "s1", "timestamp": "t0"}),
            json.dumps({
                "type": "model_change", "id": "m1",
                "provider": "nvidia", "modelId": "nemotron",
            }),
            json.dumps({
                "type": "message", "id": "a1",
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "hi"}],
                },
            }),
            json.dumps({
                "type": "weird_new_thing", "id": "u1",
                "payload": {"x": 1},
            }),
        ]) + "\n",
        encoding="utf-8",
    )
    rows = trajectory_view.project_sessions_dir(sessions)
    kinds = [r["kind"] for r in rows]
    assert "annotation" in kinds
    assert "turn" in kinds
    assert "unknown" in kinds
    unk = next(r for r in rows if r["kind"] == "unknown")
    assert unk["label"] == "weird_new_thing"
    assert unk["raw"]["type"] == "weird_new_thing"


def test_project_merges_tool_results(tmp_path: Path):
    trajectory_view.clear_cache()
    sessions = tmp_path / "sessions"
    sessions.mkdir()
    (sessions / "a.jsonl").write_text(
        "\n".join([
            json.dumps({
                "type": "message", "id": "a1",
                "message": {
                    "role": "assistant",
                    "content": [{
                        "type": "toolCall",
                        "id": "tc1",
                        "name": "read",
                        "arguments": {"path": "x"},
                    }],
                },
            }),
            json.dumps({
                "type": "message", "id": "tr1",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "tc1",
                    "content": [{"type": "text", "text": "file body"}],
                },
            }),
        ]) + "\n",
        encoding="utf-8",
    )
    rows = trajectory_view.project_sessions_dir(sessions)
    turns = [r for r in rows if r["kind"] == "turn"]
    assert len(turns) == 1
    assert turns[0]["tool_blocks"][0]["output"] == "file body"


def test_ansi_to_html_preserves_colour():
    raw = "\x1b[31mred\x1b[0m plain"
    html = git_utils.ansi_to_html(raw)
    assert '<span style="color:#c22">red</span>' in html
    assert "plain" in html
    assert "&lt;" not in html or "red" in html


def test_merge_exchange_sidecars_interleaves_errors_by_time():
    """Errors with ``at`` between turn timestamps appear in order, not at the end."""
    projected = [
        {
            "kind": "session_user",
            "id": "u1",
            "text": "first prompt",
            "timestamp": "2026-01-01T10:00:00Z",
        },
        {
            "kind": "turn",
            "id": "t1",
            "text": "reply one",
            "timestamp": "2026-01-01T10:01:00Z",
        },
        {
            "kind": "session_user",
            "id": "u2",
            "text": "second prompt",
            "timestamp": "2026-01-01T10:02:00Z",
        },
        {
            "kind": "turn",
            "id": "t2",
            "text": "reply two",
            "timestamp": "2026-01-01T10:03:00Z",
        },
    ]
    exchanges = [
        {
            "user": "first prompt",
            "errors": [
                {
                    "error": "hop failed",
                    "detail": "timeout",
                    "at": 1767261660.5,  # between turn 1 (10:01:00) and user 2 (10:02:00)
                },
            ],
        },
        {"user": "second prompt"},
    ]
    rows = trajectory_view.merge_exchange_sidecars(projected, exchanges)
    kinds = [r["kind"] for r in rows]
    assert kinds == ["user_prompt", "turn", "error", "user_prompt", "turn"]
    err = next(r for r in rows if r["kind"] == "error")
    assert err["error"] == "hop failed"
    err_idx = kinds.index("error")
    assert kinds[err_idx - 1] == "turn"
    assert kinds[err_idx + 1] == "user_prompt"


def test_host_worktree_dirty_detects_untracked(tmp_path: Path):
    import subprocess
    subprocess.run(["git", "init"], cwd=tmp_path, check=True, capture_output=True)
    subprocess.run(
        ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit",
         "--allow-empty", "-m", "init"],
        cwd=tmp_path, check=True, capture_output=True,
    )
    assert not git_utils.host_worktree_dirty(tmp_path)
    (tmp_path / "dirt.txt").write_text("x")
    assert git_utils.host_worktree_dirty(tmp_path)
