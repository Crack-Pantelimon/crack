"""Unit tests for Plan 1 UI rendering helpers (trajectory table, spawn/todo rows, context meter)."""

from __future__ import annotations

import json
from pathlib import Path

from crack_server import context_stats, paths, render


def test_render_actions_table_has_colgroup():
    html = render.render_actions_table([
        {"tool_blocks": [{"name": "bash", "input": {"command": "ls"}, "output": "ok"}]},
    ])
    assert "<colgroup>" in html
    assert "col-path" in html
    assert "col-time" in html
    assert "<th>Time</th>" in html


def test_text_row_renders_markdown_clamp_and_collapse():
    text = "\n".join(f"line **{i}**" for i in range(12))
    html = render._render_text_action_row("text", text)
    assert "md-clamp" in html
    assert "<strong>" in html or "<p>" in html  # markdown rendered
    assert "md-clamp-ellipsis" in html
    assert "md-clamp-more" in html
    assert "md-clamp-full--bordered" in html
    assert "md-collapse-btn" in html
    assert "Collapse ^" in html
    assert "<th>Time</th>" not in html  # row only; table adds header
    assert html.count("<td>") == 4  # type, path, size, time


def test_think_row_uses_same_clamped_markdown():
    thinking = "\n".join(f"thought {i}" for i in range(8))
    html = render._render_text_action_row("think", thinking)
    assert ">think<" in html or ">think</td>" in html or html.startswith("<tr><td>think</td>")
    assert "md-clamp" in html
    assert "md-collapse-btn" in html


def test_time_column_on_first_row_only():
    state = render.new_model_state()
    turns = [
        {"text": "first", "thinking": "", "tool_blocks": [], "at": 100.0, "model": "m"},
        {"text": "second", "thinking": "", "tool_blocks": [], "at": 112.5, "model": "m"},
    ]
    msgs = render.render_turn_msgs(turns, model_state=state)
    assert len(msgs) == 2
    assert "12.5s" in msgs[1]
    # First turn has no predecessor → empty Time cell (no seconds value in first row's last td).
    assert "<th>Time</th>" in msgs[0]
    # Only the first data row of the second turn carries the delta.
    assert msgs[1].count("12.5s") == 1


def test_spawn_coder_row_pretty_renders():
    instructions = "# Big task\n" + "\n".join(f"line {i}" for i in range(20))
    html = render._render_tool_action_row({
        "name": "spawn_coder",
        "input": {"instructions": instructions, "plan": True},
        "output": "Spawned coder run X",
    })
    assert "plan on" in html
    assert "md-clamp-more" in html
    assert '{"instructions"' not in html
    assert "Spawned coder run X" in html


def test_todo_row_renders_markdown():
    html = render._render_tool_action_row({
        "name": "todo",
        "input": {},
        "output": "[x] #1 done\n[ ] #2 open",
    })
    assert "todo-render" in html
    assert "<pre" not in html


def _write_cursor_session(sessions_dir: Path) -> None:
    sessions_dir.mkdir(parents=True, exist_ok=True)
    session = sessions_dir / "chat.jsonl"
    lines = [
        json.dumps({
            "message": {
                "role": "user",
                "content": "x" * 400,
            },
        }),
        json.dumps({
            "message": {
                "role": "assistant",
                "model": "cursor-agent/composer-2.5",
                "content": "hello",
                "usage": {
                    "input": 0,
                    "cacheRead": 0,
                    "output": 42,
                    "totalTokens": 42,
                    "cost": {"total": 0},
                },
            },
        }),
    ]
    session.write_text("\n".join(lines) + "\n", encoding="utf-8")


def test_session_usage_estimates_for_cursor_driver(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    sessions_dir = tmp_path / "sessions"
    _write_cursor_session(sessions_dir)

    usage = context_stats.session_usage(sessions_dir)
    assert usage is not None
    assert usage["estimated"] is True
    assert usage["tokens"] > 0
    assert usage["output"] == 42


def test_session_usage_exact_when_input_reported(tmp_path):
    sessions_dir = tmp_path / "sessions"
    sessions_dir.mkdir()
    session = sessions_dir / "chat.jsonl"
    session.write_text(
        json.dumps({
            "message": {
                "role": "assistant",
                "model": "nvidia/nemotron-3-nano-30b-a3b",
                "content": "hi",
                "usage": {
                    "input": 5000,
                    "cacheRead": 1000,
                    "output": 200,
                    "totalTokens": 6200,
                    "cost": {"total": 0.0123},
                },
            },
        })
        + "\n",
        encoding="utf-8",
    )

    usage = context_stats.session_usage(sessions_dir)
    assert usage is not None
    assert "estimated" not in usage
    assert usage["tokens"] == 6000


def test_session_usage_caches_unchanged_session(tmp_path, monkeypatch):
    sessions_dir = tmp_path / "sessions"
    sessions_dir.mkdir()
    session = sessions_dir / "chat.jsonl"
    session.write_text(
        json.dumps({
            "message": {
                "role": "assistant",
                "model": "nvidia/nemotron-3-nano-30b-a3b",
                "content": "hi",
                "usage": {
                    "input": 1000,
                    "cacheRead": 0,
                    "output": 50,
                    "totalTokens": 1050,
                    "cost": {"total": 0},
                },
            },
        })
        + "\n",
        encoding="utf-8",
    )
    calls = {"n": 0}
    original = context_stats._read_tail_lines

    def counting_read(path: Path) -> list[str]:
        calls["n"] += 1
        return original(path)

    monkeypatch.setattr(context_stats, "_read_tail_lines", counting_read)
    context_stats._USAGE_CACHE.clear()

    assert context_stats.session_usage(sessions_dir) is not None
    assert context_stats.session_usage(sessions_dir) is not None
    assert calls["n"] == 1

    session.write_text(session.read_text() + "\n", encoding="utf-8")
    assert context_stats.session_usage(sessions_dir) is not None
    assert calls["n"] == 2


def test_render_context_line_cursor_estimated_no_dollar(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    paths.models_cache_state().write({
        "info": {
            "cursor-agent/composer-2.5": {
                "context_tokens": 200_000,
                "provider": "cursor-agent",
            },
        },
    })
    sessions_dir = tmp_path / "sessions"
    _write_cursor_session(sessions_dir)

    html = context_stats.render_context_line(
        sessions_dir, "cursor-agent/composer-2.5"
    )
    assert "~" in html
    assert "200.0k" in html
    assert "$" not in html


def test_render_context_line_shows_cost_when_nonzero(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    sessions_dir = tmp_path / "sessions"
    sessions_dir.mkdir()
    session = sessions_dir / "chat.jsonl"
    session.write_text(
        json.dumps({
            "message": {
                "role": "assistant",
                "model": "nvidia/nemotron-3-nano-30b-a3b",
                "content": "hi",
                "usage": {
                    "input": 1000,
                    "cacheRead": 0,
                    "output": 50,
                    "totalTokens": 1050,
                    "cost": {"total": 0.0042},
                },
            },
        })
        + "\n",
        encoding="utf-8",
    )
    paths.models_cache_state().write({
        "info": {
            "nvidia/nemotron-3-nano-30b-a3b": {"context_tokens": 128_000},
        },
    })

    html = context_stats.render_context_line(sessions_dir)
    assert "~" not in html
    assert "$0.0042" in html
