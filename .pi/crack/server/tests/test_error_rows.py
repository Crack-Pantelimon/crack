"""Durable error rows: the error_recorder closure, the retry budget grant, and
the trajectory rendering that interleaves error rows with turns by timestamp.

Workstream B of the retry revamp: every failed pi attempt is appended to the
stage/chat ``errors`` list as a ``{"kind": "error", "at": ...}`` row (UI-only —
agent context never reads it), and ``render_turn_msgs`` merges those rows into
the trajectory in time order so the append-only delta-swap stays consistent.
"""

from __future__ import annotations

from crack_server import ratelimit, render, steprun
from crack_server.state import JsonState


def _entry(**kw) -> dict:
    base = {"message": "pi exited 1", "detail": "", "rc": 1, "attempt": 1, "phase": "test"}
    base.update(kw)
    return base


# ---------------------------------------------------------------------------
# error_recorder
# ---------------------------------------------------------------------------


def test_error_recorder_appends_timestamped_rows_and_counts(tmp_path):
    state = JsonState(tmp_path / "state.json")
    record = steprun.error_recorder(state)
    assert record(_entry(message="pi exited -9", rc=-9)) == 1
    assert record(_entry(attempt=2)) == 2

    rows = state.read()["errors"]
    assert len(rows) == 2
    assert rows[0]["kind"] == "error"
    assert rows[0]["message"] == "pi exited -9"
    assert rows[0]["rc"] == -9
    assert isinstance(rows[0]["at"], float) and rows[0]["at"] > 0
    assert rows[1]["attempt"] == 2


def test_error_recorder_subpath_targets_nested_exchange(tmp_path):
    state = JsonState(tmp_path / "chat.json")
    state.write({"exchanges": [{"user": "hi", "turns": []}]})
    record = steprun.error_recorder(state, subpath=["exchanges", 0])
    assert record(_entry(message="boom")) == 1
    assert state.read()["exchanges"][0]["errors"][0]["message"] == "boom"


def test_grant_error_budget_extends_by_max_and_keeps_rows():
    state = {
        "errors": [{"kind": "error"}] * 20,
        "error_budget": 20,
        "error_over_budget": True,
    }
    steprun.grant_error_budget(state)
    assert state["error_budget"] == 20 + ratelimit.MAX_TOTAL_ERRORS
    assert state["error_over_budget"] is False
    assert len(state["errors"]) == 20  # durable rows are never cleared


def test_make_turn_stamps_at():
    turn = steprun.make_turn({"text": "hi", "thinking": "", "tool_blocks": []}, hop=1)
    assert isinstance(turn["at"], float) and turn["at"] > 0


# ---------------------------------------------------------------------------
# render: interleaving + the fatal banner
# ---------------------------------------------------------------------------


def test_render_turn_msgs_interleaves_errors_by_timestamp():
    turns = [
        {"hop": 1, "text": "first turn", "thinking": "", "tool_blocks": [], "at": 100.0},
        {"hop": 1, "text": "second turn", "thinking": "", "tool_blocks": [], "at": 300.0},
    ]
    errors = [
        {"kind": "error", "at": 200.0, "message": "pi exited -9", "detail": "",
         "rc": -9, "attempt": 2, "phase": "test"},
    ]
    msgs = render.render_turn_msgs(turns, errors=errors)
    assert len(msgs) == 3
    assert "first turn" in msgs[0]
    assert "⚠ pi exited -9" in msgs[1]
    assert "attempt 2" in msgs[1]
    assert "second turn" in msgs[2]


def test_render_turn_msgs_legacy_turns_keep_list_order():
    # Turns without `at` (pre-timestamp states) keep their list order; the
    # error row sorts after the turns it follows.
    turns = [
        {"hop": 1, "text": "legacy one", "thinking": "", "tool_blocks": []},
        {"hop": 2, "text": "legacy two", "thinking": "", "tool_blocks": []},
    ]
    errors = [{"kind": "error", "at": 1.0, "message": "boom"}]
    msgs = render.render_turn_msgs(turns, errors=errors)
    assert len(msgs) == 3
    assert "legacy one" in msgs[0]
    assert "legacy two" in msgs[1]
    assert "⚠ boom" in msgs[2]


def test_render_turn_msgs_without_errors_is_unchanged():
    turns = [{"hop": 1, "text": "plain turn", "thinking": "", "tool_blocks": []}]
    assert render.render_turn_msgs(turns) == render.render_turn_msgs(turns, errors=[])


def test_render_error_row_shows_attempt_detail_and_time():
    html = render.render_error_row(
        {"kind": "error", "at": 100.0, "message": "pi exited 1",
         "detail": "last stderr:\nboom", "rc": 1, "attempt": 3, "phase": "test"}
    )
    assert "stage-msg stage-error" in html
    assert "⚠ pi exited 1" in html
    assert "attempt 3" in html
    assert "ago" in html  # relative time from `at`
    assert "last stderr:\nboom" in html


def test_render_fatal_error_banner():
    assert render.render_fatal_error_banner({}) == ""
    assert render.render_fatal_error_banner({"errors": []}) == ""
    assert render.render_fatal_error_banner({"errors": [{}] * 19}) == ""
    # Under a raised budget (a manual continue granted more), 20 rows is fine.
    assert render.render_fatal_error_banner({"errors": [{}] * 20, "error_budget": 40}) == ""

    html = render.render_fatal_error_banner({"error_over_budget": True})
    assert "stage-error--fatal" in html
    assert "something is likely wrong" in html

    html = render.render_fatal_error_banner({"errors": [{}] * 20})
    assert "Failed more than 20 times" in html


def test_render_error_stop_row_includes_duration():
    html = render.render_error_stop_row(42.3)
    assert "stage-error" in html
    assert "Stopped: Error after 42.3s" in html


def test_errored_chat_emits_stopped_error_line(tmp_path, monkeypatch):
    """Errored chats (phase idle + error set) show the red runtime line."""
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    from crack_server import chats, paths

    chat_id = "1700000000000"
    paths.chat_dir(chat_id).mkdir(parents=True)
    paths.chat_sessions_dir(chat_id).mkdir(parents=True)
    paths.chat_info_state(chat_id).write({"title": "t", "model": "m"})
    paths.chat_state(chat_id).write({
        "phase": "idle",
        "error": "429 Too Many Requests",
        "exchanges": [{
            "user": "hi",
            "turns": [
                {"at": 100.0, "text": "a", "thinking": "", "tool_blocks": []},
                {"at": 142.3, "text": "b", "thinking": "", "tool_blocks": []},
            ],
            "started_at": 100.0,
            "finished_at": 142.3,
            "stop_reason": "empty",
        }],
    })
    msgs = chats.render_chat_msgs(chat_id)
    joined = "".join(msgs)
    assert "Stopped: Error after" in joined
    assert "stage-error" in joined
