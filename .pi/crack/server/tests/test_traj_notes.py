"""Trajectory notes + single-delivery ledger.

Covers the three fixes from the child-return/patch-trace work:
  * finish() records a live ``child_return`` note in the parent trajectory;
  * the ``delivered_to_parent`` ledger stops a report delivered inline (wait_join)
    from being rolled again by the deferred child_report merge — including the
    finish()-gap rebuild race;
  * patch build/apply emit ``patch`` notes with file/byte stats.
"""

from __future__ import annotations

import time

import pytest

from crack_server import patch, paths
from crack_server import render as render_mod
from crack_server import trajectory_view
from crack_server.sub_agents import runner, wait
from tests.test_sub_agents import _seed_personas, chat_root, fake_pi  # noqa: F401


# ---------------------------------------------------------------------------
# child_return note
# ---------------------------------------------------------------------------


def test_finish_records_child_return_note(chat_root):
    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    runner.finish(run["run_id"], "done")
    notes = paths.chat_state(chat_root).read().get("traj_notes") or []
    child_returns = [n for n in notes if n.get("note_type") == "child_return"]
    assert len(child_returns) == 1
    note = child_returns[0]
    assert note["kind"] == "note"
    assert note["status"] == "ok"
    assert "returned" in note["text"]
    assert "coder" in note["text"]
    assert isinstance(note["at"], float)


def test_finish_error_note_status(chat_root):
    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    runner.finish(run["run_id"], "error")
    note = [
        n for n in paths.chat_state(chat_root).read()["traj_notes"]
        if n.get("note_type") == "child_return"
    ][0]
    assert note["status"] == "err"


# ---------------------------------------------------------------------------
# delivered_to_parent ledger
# ---------------------------------------------------------------------------


def test_merge_drops_already_delivered(chat_root):
    from crack_server import chats

    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    runner.finish(run["run_id"], "done")
    # Report reached the agent inline (wait_join) → ledger set.
    runner.mark_delivered_to_parent(run["run_id"])
    assert paths.chat_state(chat_root).read().get("child_inbox")

    merged = chats._merge_child_inbox(chat_root)
    assert merged == 0
    state = paths.chat_state(chat_root).read()
    assert not [
        p for p in (state.get("pending") or []) if p.get("source") == "child_report"
    ]
    assert state.get("child_inbox") in (None, [])


def test_merge_delivers_once_then_noops(chat_root):
    from crack_server import chats

    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    runner.finish(run["run_id"], "done")

    assert chats._merge_child_inbox(chat_root) == 1
    pend = paths.chat_state(chat_root).read().get("pending") or []
    assert [p for p in pend if p.get("source") == "child_report"]
    # Marked delivered as it was rolled → a second merge can't duplicate it.
    assert paths.run_state_by_id(run["run_id"]).read().get("delivered_to_parent")
    assert chats._merge_child_inbox(chat_root) == 0


def test_wait_poll_marks_delivered(chat_root):
    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    runner.finish(run["run_id"], "done")
    result = wait.poll(chat_id=chat_root, parent_kind="chat", parent_id=chat_root)
    assert {r["run_id"] for r in result["results"]} == {run["run_id"]}
    assert paths.run_state_by_id(run["run_id"]).read().get("delivered_to_parent")


def test_finish_gap_rebuild_race_no_duplicate_roll(chat_root):
    """The exact double: a two-strike rebuild delivers during the finish() gap
    (parent_notified set, inbox entry not yet written), then the inbox write
    lands. The deferred merge must drop it rather than roll a duplicate."""
    from crack_server import chats

    run = runner.spawn(
        chat_id=chat_root, persona_slug="coder", instructions="X",
        parent_kind="chat", parent_id=chat_root, depth=0,
    )
    run_id = run["run_id"]
    # Simulate the gap: terminal + notified, but the inbox entry hasn't landed.
    paths.run_state_by_id(run_id).update(
        lambda s: {**s, "phase": "done", "parent_notified": True,
                   "finished_at": time.time()}
    )

    # Two-strike rebuild delivers the report inline from run state (no drain).
    result = wait.poll(
        chat_id=chat_root, parent_kind="chat", parent_id=chat_root,
        target=run_id, rebuild=[run_id],
    )
    assert [r["run_id"] for r in result["results"]] == [run_id]
    assert result["results"][0]["delivered_earlier"] is True
    assert paths.run_state_by_id(run_id).read().get("delivered_to_parent")

    # Now the delayed inbox write lands (finish()'s _inbox_chat).
    entry = runner.build_entry(run_id, status="done")
    paths.chat_state(chat_root).update(
        lambda s: {**s, "child_inbox": [*(s.get("child_inbox") or []), entry]}
    )

    assert chats._merge_child_inbox(chat_root) == 0
    assert not [
        p for p in (paths.chat_state(chat_root).read().get("pending") or [])
        if p.get("source") == "child_report"
    ]


# ---------------------------------------------------------------------------
# patch stats + notes
# ---------------------------------------------------------------------------

_SAMPLE_DIFF = """diff --git a/new.py b/new.py
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/new.py
@@ -0,0 +1,2 @@
+one
+two
diff --git a/mod.py b/mod.py
index 1111111..2222222 100644
--- a/mod.py
+++ b/mod.py
@@ -1 +1 @@
-old
+new
diff --git a/gone.py b/gone.py
deleted file mode 100644
index 3333333..0000000
--- a/gone.py
+++ /dev/null
@@ -1 +0,0 @@
-bye
"""


def test_diff_stats_counts():
    stats = patch.diff_stats(_SAMPLE_DIFF)
    assert stats["files"] == 3
    assert stats["added"] == 1
    assert stats["deleted"] == 1
    assert stats["modified"] == 1
    assert stats["bytes"] == len(_SAMPLE_DIFF.encode("utf-8"))


def test_human_bytes():
    assert patch._human_bytes(512) == "512 B"
    assert patch._human_bytes(12_340) == "12.34 KB"
    assert patch._human_bytes(2_500_000) == "2.50 MB"


def test_format_patch_summary():
    summary = patch.format_patch_summary(patch.diff_stats(_SAMPLE_DIFF))
    assert "3 files changed" in summary
    assert "1 added" in summary
    assert "1 deleted" in summary


# ---------------------------------------------------------------------------
# rendering + sidecar merge
# ---------------------------------------------------------------------------


def test_render_note_row():
    html = render_mod.render_note_row({
        "kind": "note", "note_type": "patch", "status": "err",
        "icon": "⚠", "text": "boom", "at": time.time(),
        "detail": "git apply stderr",
    })
    assert "traj-note--patch" in html
    assert "traj-note--err" in html
    assert "boom" in html
    assert "git apply stderr" in html
    assert "⚠" in html


def test_notes_merge_into_trajectory_by_time():
    projected = [
        {"kind": "session_user", "text": "hi", "timestamp": None, "at": 100.0},
        {"kind": "turn", "text": "working", "at": 101.0, "tool_blocks": []},
    ]
    notes = [
        {"kind": "note", "note_type": "child_return", "text": "returned", "at": 102.0},
    ]
    rows = trajectory_view.merge_exchange_sidecars(projected, [], notes=notes)
    assert rows[-1].get("note_type") == "child_return"
