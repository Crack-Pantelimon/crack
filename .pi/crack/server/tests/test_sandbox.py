"""Unit tests for crack_server.sandbox (podman mocked)."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, patch

import pytest

from crack_server import sandbox as s


@pytest.fixture
def host_env(monkeypatch, tmp_path):
    monkeypatch.setenv("CRACK_HOST_REPO_ROOT", str(tmp_path / "repo"))
    monkeypatch.setenv("CRACK_HARNESS_DATA_DIR", str(tmp_path / "harness"))


@pytest.mark.anyio
async def test_sandbox_enabled_off_in_tests(host_env, monkeypatch, tmp_path):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    monkeypatch.delenv("CRACK_SANDBOX_ENABLED", raising=False)
    assert not s.sandbox_enabled()


@pytest.mark.anyio
async def test_sandbox_enabled_forced(monkeypatch, host_env):
    monkeypatch.setenv("CRACK_SANDBOX_ENABLED", "1")
    assert s.sandbox_enabled()
    assert s.sandbox_name("123_abc") == "crack-sbx-123_abc"


@pytest.mark.anyio
async def test_ensure_network_creates_when_missing(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        if args == ("network", "exists", s.CRACK_NET):
            return 1, "", ""
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        await s.ensure_network()

    assert ("network", "exists", s.CRACK_NET) in calls
    assert ("network", "create", s.CRACK_NET) in calls


@pytest.mark.anyio
async def test_ensure_network_skips_create_when_present(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        await s.ensure_network()

    assert calls == [("network", "exists", s.CRACK_NET)]


@pytest.mark.anyio
async def test_ensure_sandbox_starts_existing(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        if args[:3] == ("container", "exists", "crack-sbx-cid"):
            return 0, "", ""
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        name = await s.ensure_sandbox("cid")

    assert name == "crack-sbx-cid"
    assert ("start", "crack-sbx-cid") in calls
    assert not any(a[0] == "run" for a in calls)


@pytest.mark.anyio
async def test_ensure_sandbox_creates_with_overlay_dirs(monkeypatch, tmp_path):
    repo = tmp_path / "repo"
    harness = tmp_path / "harness"
    repo.mkdir()
    harness.mkdir()
    monkeypatch.setenv("CRACK_HOST_REPO_ROOT", str(repo))
    monkeypatch.setenv("CRACK_HARNESS_DATA_DIR", str(harness))

    run_args: list[str] = []

    async def fake_podman(*args, timeout=60):
        if args[:3] == ("container", "exists", "crack-sbx-new"):
            return 1, "", ""
        if args[:2] == ("volume", "inspect"):
            return 0, "/host/vol/crack-harness-data\n", ""
        if args[0] == "run":
            run_args.extend(args)
            return 0, "container-id\n", ""
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        name = await s.ensure_sandbox("new")

    assert name == "crack-sbx-new"
    assert (harness / "overlays" / "new" / "upper").is_dir()
    assert (harness / "overlays" / "new" / "work").is_dir()
    joined = " ".join(run_args)
    assert "--network" in run_args and s.CRACK_NET in run_args
    assert "upperdir=/host/vol/crack-harness-data/overlays/new/upper" in joined
    assert "workdir=/host/vol/crack-harness-data/overlays/new/work" in joined
    assert f"{repo}:/workspace:O" in joined
    assert "CRACK_PI_HOST=crack-dev" in joined
    assert "/workspace/_docker/_sandbox_start.sh" in joined


@pytest.mark.anyio
async def test_exec_in_builds_command(host_env):
    with patch("asyncio.create_subprocess_exec", new_callable=AsyncMock) as mock_exec:
        mock_exec.return_value = object()
        proc = await s.exec_in(
            "crack-sbx-x",
            ["pi", "--version"],
            env={"FOO": "bar"},
            cwd="/workspace",
        )
        assert proc is mock_exec.return_value
        mock_exec.assert_awaited_once()
        cmd = mock_exec.call_args[0]
        assert cmd[0] == "podman"
        assert "exec" in cmd
        assert "-w" in cmd and "/workspace" in cmd
        assert "-e" in cmd and "FOO=bar" in cmd
        assert "crack-sbx-x" in cmd
        assert "pi" in cmd


@pytest.mark.anyio
async def test_kill_session_escalates_to_kill(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        if args[:4] == ("exec", "sbx", "pkill", "-TERM"):
            return 0, "", ""
        if args[:4] == ("exec", "sbx", "pgrep", "-f"):
            return 0, "123\n", ""
        if args[:4] == ("exec", "sbx", "pkill", "-KILL"):
            return 0, "", ""
        return 0, "", ""

    with (
        patch.object(s, "_KILL_GRACE_SECONDS", 0.05),
        patch.object(s, "_podman", side_effect=fake_podman),
    ):
        await s.kill_session("sbx", "sess-abc")

    assert any(a[:4] == ("exec", "sbx", "pkill", "-TERM") for a in calls)
    assert any(a[:4] == ("exec", "sbx", "pkill", "-KILL") for a in calls)


@pytest.mark.anyio
async def test_destroy_sandbox_kill_and_rm(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        await s.destroy_sandbox("gone")

    assert ("container", "exists", "crack-sbx-gone") in calls
    assert ("kill", "crack-sbx-gone") in calls
    assert ("rm", "-f", "crack-sbx-gone") in calls


@pytest.mark.anyio
async def test_destroy_sandbox_noop_when_missing(host_env):
    calls: list[tuple[str, ...]] = []

    async def fake_podman(*args, timeout=60):
        calls.append(args)
        if args[:2] == ("container", "exists"):
            return 1, "", ""
        return 0, "", ""

    with patch.object(s, "_podman", side_effect=fake_podman):
        await s.destroy_sandbox("nope")

    assert calls == [("container", "exists", "crack-sbx-nope")]
