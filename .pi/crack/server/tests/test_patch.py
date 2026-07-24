"""Unit tests for crack_server.patch (podman/git mocked)."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest

from crack_server import patch as p


@pytest.fixture
def artifact_dir(tmp_path):
    d = tmp_path / "chat123"
    d.mkdir()
    return d


@pytest.mark.anyio
async def test_capture_baseline_writes_tree(artifact_dir):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=300):
        calls.append(args)
        if args[-2:] == ("add", "-A"):
            return 0, "", ""
        if args[-1] == "write-tree":
            return 0, "abc123def\n", ""
        return 0, "", ""

    with patch.object(p.sandbox, "_podman", side_effect=fake_podman):
        tree = await p.capture_baseline("crack-sbx-x", artifact_dir)

    assert tree == "abc123def"
    assert p.base_tree_path(artifact_dir).read_text() == "abc123def\n"


@pytest.mark.anyio
async def test_extract_patch_empty_diff(artifact_dir):
    p.base_tree_path(artifact_dir).write_text("base\n")

    async def fake_podman(*args, timeout=300):
        if args[:2] == ("exec", "crack-sbx-x") and args[2] == "bash":
            return 0, "", ""
        if args[-2:] == ("read-tree", "base"):
            return 0, "", ""
        if args[-2:] == ("add", "-A"):
            return 0, "", ""
        if args[-1] == "write-tree":
            return 0, "endtree\n", ""
        if "diff" in args:
            assert "--binary" in args
            return 0, "", ""
        return 0, "", ""

    with patch.object(p.sandbox, "_podman", side_effect=fake_podman):
        result = await p.extract_patch("crack-sbx-x", artifact_dir)

    assert result.empty
    assert not result.needs_nag


@pytest.mark.anyio
async def test_produce_diff_seeds_index_from_base_tree(artifact_dir):
    """Tracked-but-gitignored files must not spuriously appear as deletions."""
    p.base_tree_path(artifact_dir).write_text("basetree\n")
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=300):
        calls.append(args)
        if args[:2] == ("exec", "crack-sbx-x") and args[2] == "bash":
            return 0, "", ""
        if args[-2:] == ("read-tree", "basetree"):
            return 0, "", ""
        if args[-2:] == ("add", "-A"):
            return 0, "", ""
        if args[-1] == "write-tree":
            return 0, "basetree\n", ""
        if "diff" in args:
            assert "--binary" in args
            return 0, "diff --git a/new.txt b/new.txt\n", ""
        return 0, "", ""

    with patch.object(p.sandbox, "_podman", side_effect=fake_podman):
        result = await p.extract_patch("crack-sbx-x", artifact_dir)

    assert result.has_content
    patch_text = p.patch_diff_path(artifact_dir).read_text()
    assert "new.txt" in patch_text
    assert "data.bytes" not in patch_text
    read_tree_idx = next(
        i for i, a in enumerate(calls) if a[-2:] == ("read-tree", "basetree")
    )
    add_idx = next(i for i, a in enumerate(calls) if a[-2:] == ("add", "-A"))
    assert read_tree_idx < add_idx


@pytest.mark.anyio
async def test_extract_patch_nag_on_big_file(artifact_dir):
    p.base_tree_path(artifact_dir).write_text("base\n")
    big_line = f"big.bin\t{p.MAX_FILE_BYTES + 1}\n"

    async def fake_podman(*args, timeout=300):
        if args[:2] == ("exec", "crack-sbx-x") and args[2] == "bash":
            return 0, big_line, ""
        if args[-1] == "reset":
            return 0, "", ""
        return 0, "", ""

    with patch.object(p.sandbox, "_podman", side_effect=fake_podman):
        result = await p.extract_patch("crack-sbx-x", artifact_dir, nag_attempt=0)

    assert result.needs_nag
    assert result.big_files == (("/workspace/big.bin", p.MAX_FILE_BYTES + 1),)


def test_format_big_file_nag_lists_paths():
    text = p.format_big_file_nag((("/workspace/big.bin", 120_000_000),))
    assert "/workspace/big.bin" in text
    assert "120000000" in text


def test_format_apply_failure_includes_patch_path(tmp_path):
    patch_file = tmp_path / "patch.diff"
    text = p.format_apply_failure("conflict", patch_file)
    assert "Patch application failed" in text
    assert str(patch_file.resolve()) in text
    assert "Resolve the conflict" in text


def test_record_chat_apply_failure_sets_error_without_enqueue(tmp_path, monkeypatch):
    from crack_server import chats, paths, queue

    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, "nvidia/z-ai/glm-5.2")
    patch_file = paths.chat_dir(chat_id) / "patch.diff"
    patch_file.write_text("diff\n")

    enqueued: list[str] = []
    monkeypatch.setattr(
        queue, "enqueue_exclusive",
        lambda *a, **k: enqueued.append(a[0]) or "job-id",
    )

    p.record_chat_apply_failure(chat_id, "boom", patch_file)

    state = paths.chat_state(chat_id).read()
    assert state["phase"] == "idle"
    assert "git apply failed" in state["error"]
    assert "boom" in state["error_detail"]
    assert str(patch_file.resolve()) in state["error_detail"]
    assert not state.get("pending")
    assert not any(e.get("source") == "patch_apply" for e in state.get("exchanges") or [])
    assert enqueued == []
    assert not queue.has_job(chat_id, chats.CHAT_JOB_SLUG)


@pytest.mark.anyio
async def test_publish_pending_patch_enters_review(tmp_path, monkeypatch):
    """Finished turn publishes a pending patch and stops the container (no host apply)."""
    from crack_server import chats, paths, queue

    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, "nvidia/z-ai/glm-5.2")
    artifact_dir = paths.chat_dir(chat_id)
    patch_file = p.patch_diff_path(artifact_dir)
    patch_file.write_text("diff --git a/x.txt b/x.txt\n")

    enqueued: list[str] = []
    monkeypatch.setattr(
        queue, "enqueue_exclusive",
        lambda *a, **k: enqueued.append(a[0]) or "job-id",
    )
    monkeypatch.setattr(
        p, "extract_patch",
        AsyncMock(return_value=p.ExtractResult(
            patch_path=patch_file, empty=False, needs_nag=False,
            big_files=(), nag_attempt=0,
        )),
    )
    stop = AsyncMock()
    destroy = AsyncMock()
    monkeypatch.setattr(p.sandbox, "stop_sandbox", stop)
    monkeypatch.setattr(p.sandbox, "destroy_sandbox", destroy)
    monkeypatch.setattr(p, "apply_patch_on_host", AsyncMock(return_value=(False, "boom")))

    await p.finalize_chat_sandbox(chat_id, f"crack-sbx-{chat_id}")

    state = paths.chat_state(chat_id).read()
    assert state["phase"] == "review"
    assert state.get("pending_patch")
    assert not state.get("error")
    assert enqueued == []
    assert not queue.has_job(chat_id, chats.CHAT_JOB_SLUG)
    stop.assert_awaited_once()
    destroy.assert_not_awaited()


def test_notify_parent_apply_failure_chat_records_error_not_enqueues(tmp_path, monkeypatch):
    from crack_server import chats, paths, queue

    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, "nvidia/z-ai/glm-5.2")
    patch_file = tmp_path / "patch.diff"

    enqueued: list[str] = []
    monkeypatch.setattr(
        queue, "enqueue_exclusive",
        lambda *a, **k: enqueued.append(a[0]) or "job-id",
    )

    p.notify_parent_apply_failure("chat", chat_id, chat_id, "conflict", patch_file)

    state = paths.chat_state(chat_id).read()
    assert state["phase"] == "idle"
    assert state.get("error")
    assert not state.get("pending")
    assert enqueued == []
    assert not queue.has_job(chat_id, chats.CHAT_JOB_SLUG)


# -- Plan 7 / parallel-patch guard: dispatch-ordered, serialized drain --------


def _make_child(chat_id, run_id, *, pending=True):
    from crack_server import paths

    paths.run_dir(chat_id, run_id).mkdir(parents=True, exist_ok=True)
    paths.run_state(chat_id, run_id).write({
        "run_id": run_id,
        "chat_id": chat_id,
        "parent_kind": "chat",
        "parent_id": chat_id,
        "phase": "done",
        "patch_pending": pending,
    })
    p.patch_diff_path(paths.run_dir(chat_id, run_id)).write_text(f"patch-for-{run_id}\n")


@pytest.fixture
def drain_chat(tmp_path, monkeypatch):
    from crack_server import paths

    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, "nvidia/z-ai/glm-5.2")
    return chat_id


def test_drain_applies_in_dispatch_order(drain_chat, monkeypatch):
    from crack_server import paths

    older = paths.generate_run_id()
    newer = paths.generate_run_id()
    while newer <= older:
        newer = paths.generate_run_id()
    # Create out of spawn order to prove the drain sorts, not insertion order.
    _make_child(drain_chat, newer)
    _make_child(drain_chat, older)

    applied: list[str] = []
    monkeypatch.setattr(p.sandbox, "sandbox_enabled", lambda: True)
    monkeypatch.setattr(p, "_provenance_commit_in_sandbox", lambda *a, **k: None)

    def fake_merge(sbx, artifact_dir):
        patch_path = p.patch_diff_path(artifact_dir)
        applied.append(patch_path.read_text().strip())
        return p.MergeResult(ok=True, changed_paths=("x",))

    monkeypatch.setattr(p, "merge_apply_sync", fake_merge)

    p.drain_parent_patches(drain_chat, "chat", drain_chat)

    assert applied == [f"patch-for-{older}", f"patch-for-{newer}"]
    for rid in (older, newer):
        assert paths.run_state(drain_chat, rid).read().get("patch_pending") is False
    assert paths.chat_state(drain_chat).read().get("patch_draining") is False


def test_drain_applies_while_siblings_running(drain_chat, monkeypatch):
    """Finished-child patches merge immediately; running siblings must not defer."""
    from crack_server import paths
    from crack_server.sub_agents import runner

    rid = paths.generate_run_id()
    _make_child(drain_chat, rid)

    applied: list[str] = []
    monkeypatch.setattr(p.sandbox, "sandbox_enabled", lambda: True)
    # Even if active_child_count claims a sibling is alive, drain must proceed.
    monkeypatch.setattr(runner, "active_child_count", lambda *a, **k: 1)
    monkeypatch.setattr(p, "_provenance_commit_in_sandbox", lambda *a, **k: None)
    monkeypatch.setattr(
        p, "merge_apply_sync",
        lambda s, ad: (applied.append(p.patch_diff_path(ad).read_text().strip()),
                       p.MergeResult(ok=True, changed_paths=("x",)))[1],
    )

    p.drain_parent_patches(drain_chat, "chat", drain_chat)

    assert applied == [f"patch-for-{rid}"]
    assert paths.run_state(drain_chat, rid).read().get("patch_pending") is False


def test_drain_conflict_notifies_and_clears(drain_chat, monkeypatch):
    from crack_server import paths

    rid = paths.generate_run_id()
    _make_child(drain_chat, rid)

    notified: list[str] = []
    monkeypatch.setattr(p.sandbox, "sandbox_enabled", lambda: True)
    monkeypatch.setattr(
        p, "merge_apply_sync",
        lambda s, ad: p.MergeResult(ok=False, err="conflict"),
    )
    monkeypatch.setattr(
        p, "notify_parent_apply_failure",
        lambda pk, pi, cid, err, pp: notified.append(err),
    )

    p.drain_parent_patches(drain_chat, "chat", drain_chat)

    assert notified == ["conflict"]
    # Cleared even on conflict, so a later sibling isn't blocked behind it.
    assert paths.run_state(drain_chat, rid).read().get("patch_pending") is False


def test_drain_apply_exception_leaves_pending(drain_chat, monkeypatch):
    from crack_server import paths

    rid = paths.generate_run_id()
    _make_child(drain_chat, rid)

    monkeypatch.setattr(p.sandbox, "sandbox_enabled", lambda: True)

    def boom(sbx, ad):
        raise RuntimeError("podman timed out")

    monkeypatch.setattr(p, "merge_apply_sync", boom)

    p.drain_parent_patches(drain_chat, "chat", drain_chat)

    # A raised merge must NOT clear the flag (so a later drain retries it) and must
    # release the drain lock.
    assert paths.run_state(drain_chat, rid).read().get("patch_pending") is True
    assert paths.chat_state(drain_chat).read().get("patch_draining") is False
