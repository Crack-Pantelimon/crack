"""Tests for merge-tree based patch integration (apply-twice plan)."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

from crack_server import patch as p


def _git(cwd: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(cwd), *args],
        capture_output=True, text=True, check=check,
    )


def _init_repo(repo: Path) -> None:
    repo.mkdir(parents=True, exist_ok=True)
    _git(repo, "init", "-q")
    _git(repo, "config", "user.email", "t@t")
    _git(repo, "config", "user.name", "t")
    (repo / "a.txt").write_text("base\n", encoding="utf-8")
    _git(repo, "add", "a.txt")
    _git(repo, "commit", "-q", "-m", "base")


def _rev(repo: Path, rev: str = "HEAD") -> str:
    return _git(repo, "rev-parse", rev).stdout.strip()


def _tree(repo: Path, rev: str = "HEAD") -> str:
    return _git(repo, "rev-parse", f"{rev}^{{tree}}").stdout.strip()


def _clone(src: Path, dest: Path) -> None:
    if dest.exists():
        shutil.rmtree(dest)
    _git(src, "clone", "-q", str(src), str(dest))
    _git(dest, "config", "user.email", "t@t")
    _git(dest, "config", "user.name", "t")


def _write_bundle_from_repo(
    producer: Path,
    artifact_dir: Path,
    *,
    base_commit: str,
    base_tree: str,
) -> None:
    """Build delta.bundle+delta.json+patch.diff from a producer repo tip."""
    artifact_dir.mkdir(parents=True, exist_ok=True)
    end_commit = _rev(producer)
    end_tree = _tree(producer)
    log = _git(producer, "rev-list", "--count", f"{base_commit}..{end_commit}")
    assert int(log.stdout.strip()) >= 1, (
        f"empty range {base_commit[:8]}..{end_commit[:8]}"
    )
    bundle = artifact_dir / "delta.bundle"
    _git(producer, "update-ref", "refs/crack/delta-base", base_commit)
    _git(producer, "update-ref", "refs/crack/delta-end", end_commit)
    try:
        proc = _git(
            producer, "bundle", "create", str(bundle),
            "refs/crack/delta-base..refs/crack/delta-end",
            check=False,
        )
    finally:
        _git(producer, "update-ref", "-d", "refs/crack/delta-base", check=False)
        _git(producer, "update-ref", "-d", "refs/crack/delta-end", check=False)
    if proc.returncode != 0:
        raise AssertionError(
            f"bundle create failed: {(proc.stderr or proc.stdout).strip()}"
        )
    (artifact_dir / "delta.json").write_text(
        json.dumps({
            "base_commit": base_commit,
            "base_tree": base_tree,
            "end_commit": end_commit,
            "end_tree": end_tree,
        })
        + "\n",
        encoding="utf-8",
    )
    diff = _git(producer, "diff", "--binary", base_tree, end_tree).stdout
    (artifact_dir / "patch.diff").write_text(diff, encoding="utf-8")


@pytest.fixture
def host_repo(tmp_path, monkeypatch):
    """Temp git repo that merge_apply treats as WORKSPACE via _git_host."""
    repo = tmp_path / "host"
    _init_repo(repo)
    monkeypatch.setattr(p, "WORKSPACE", str(repo))

    def fake_git_host(*args, timeout=300.0):
        try:
            proc = subprocess.run(
                ["git", "-C", str(repo), *args],
                capture_output=True, timeout=timeout, check=False,
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError("git timed out") from None
        out = (proc.stdout or b"").decode("utf-8", "replace")
        err = (proc.stderr or b"").decode("utf-8", "replace")
        return proc.returncode if proc.returncode is not None else -1, out, err

    monkeypatch.setattr(p, "_git_host", fake_git_host)
    return repo


@pytest.mark.anyio
async def test_merge_apply_no_drift(host_repo, tmp_path):
    """Delta onto unchanged dest ≡ end tree contents."""
    base_commit = _rev(host_repo)
    base_tree = _tree(host_repo)

    producer = tmp_path / "producer"
    _clone(host_repo, producer)
    (producer / "b.txt").write_text("new\n", encoding="utf-8")
    _git(producer, "add", "b.txt")
    _git(producer, "commit", "-q", "-m", "add b")

    artifact = tmp_path / "art"
    _write_bundle_from_repo(
        producer, artifact, base_commit=base_commit, base_tree=base_tree,
    )

    result = await p.merge_apply(None, artifact)
    assert result.ok, result.err
    assert not result.used_fallback
    assert (host_repo / "b.txt").read_text(encoding="utf-8") == "new\n"
    assert _rev(host_repo) == base_commit
    assert "b.txt" in result.changed_paths


@pytest.mark.anyio
async def test_merge_apply_exact_bug_identical_plan(host_repo, tmp_path):
    """Base has no plan; dest has plan committed; end re-adds same plan + code."""
    base_commit = _rev(host_repo)
    base_tree = _tree(host_repo)

    (host_repo / "plan.md").write_text("plan v1\n", encoding="utf-8")
    _git(host_repo, "add", "plan.md")
    _git(host_repo, "commit", "-q", "-m", "plan")
    dest_head = _rev(host_repo)

    producer = tmp_path / "producer"
    _clone(host_repo, producer)
    _git(producer, "reset", "--hard", base_commit)
    (producer / "plan.md").write_text("plan v1\n", encoding="utf-8")
    (producer / "code.py").write_text("print(1)\n", encoding="utf-8")
    _git(producer, "add", "plan.md", "code.py")
    _git(producer, "commit", "-q", "-m", "agent")

    artifact = tmp_path / "art"
    _write_bundle_from_repo(
        producer, artifact, base_commit=base_commit, base_tree=base_tree,
    )

    result = await p.merge_apply(None, artifact)
    assert result.ok, result.err
    assert (host_repo / "plan.md").read_text(encoding="utf-8") == "plan v1\n"
    assert (host_repo / "code.py").read_text(encoding="utf-8") == "print(1)\n"
    assert _rev(host_repo) == dest_head


@pytest.mark.anyio
async def test_merge_apply_real_conflict(host_repo, tmp_path):
    base_commit = _rev(host_repo)
    base_tree = _tree(host_repo)

    (host_repo / "a.txt").write_text("host edit\n", encoding="utf-8")
    _git(host_repo, "add", "a.txt")
    _git(host_repo, "commit", "-q", "-m", "host")

    producer = tmp_path / "producer"
    _clone(host_repo, producer)
    _git(producer, "reset", "--hard", base_commit)
    (producer / "a.txt").write_text("agent edit\n", encoding="utf-8")
    _git(producer, "add", "a.txt")
    _git(producer, "commit", "-q", "-m", "agent")

    artifact = tmp_path / "art"
    _write_bundle_from_repo(
        producer, artifact, base_commit=base_commit, base_tree=base_tree,
    )

    result = await p.merge_apply(None, artifact)
    assert not result.ok
    assert result.conflicted_paths
    assert "a.txt" in result.conflicted_paths


@pytest.mark.anyio
async def test_merge_apply_fallback_missing_bundle(host_repo, tmp_path, monkeypatch):
    artifact = tmp_path / "art"
    artifact.mkdir()

    producer = tmp_path / "producer"
    _clone(host_repo, producer)
    (producer / "x.txt").write_text("x\n", encoding="utf-8")
    _git(producer, "add", "x.txt")
    diff = _git(producer, "diff", "--binary", "--cached", "HEAD").stdout
    (artifact / "patch.diff").write_text(diff, encoding="utf-8")

    calls = {"n": 0}
    real_apply = p._apply_git

    async def spy_apply(target, path):
        calls["n"] += 1
        return await real_apply(target, path)

    monkeypatch.setattr(p, "_apply_git", spy_apply)
    result = await p.merge_apply(None, artifact)
    assert result.used_fallback
    assert calls["n"] == 1
    assert result.ok, result.err
    assert (host_repo / "x.txt").read_text(encoding="utf-8") == "x\n"


def test_host_apply_lock_creates_file(tmp_path, monkeypatch):
    from crack_server import paths

    monkeypatch.setenv("CRACK_HARNESS_DATA_DIR", str(tmp_path / "harness"))
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    with p.host_apply_lock():
        lock = paths.harness_dir() / "locks" / "host-apply.lock"
        assert lock.is_file()


def test_parse_merge_tree_conflict_paths():
    out = (
        "abc123deadbeefabc123deadbeefabc123deadbeef\n"
        "100644 aaa 1\ta.txt\n"
        "Auto-merging a.txt\n"
        "CONFLICT (content): Merge conflict in a.txt\n"
    )
    tree, paths, msgs = p._parse_merge_tree_output(out)
    assert tree.startswith("abc123")
    assert paths == ("a.txt",)
    assert "CONFLICT" in msgs


def test_serialize_review_comments():
    s = p._serialize_review_comments([
        {"file": "foo.py", "side": "new", "line": 3, "body": "nit"},
        {"file": "bar.py", "line": 1, "body": "fix"},
    ])
    assert "foo.py:3: nit" in s
    assert "bar.py:1: fix" in s
