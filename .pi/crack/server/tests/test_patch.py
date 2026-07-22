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
        if args[-2:] == ("add", "-A"):
            return 0, "", ""
        if args[-1] == "write-tree":
            return 0, "endtree\n", ""
        if args[-3:-1] == ("diff", "base"):
            return 0, "", ""
        return 0, "", ""

    with patch.object(p.sandbox, "_podman", side_effect=fake_podman):
        result = await p.extract_patch("crack-sbx-x", artifact_dir)

    assert result.empty
    assert not result.needs_nag


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
