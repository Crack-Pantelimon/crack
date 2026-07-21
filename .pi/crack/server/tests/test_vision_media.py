"""Vision (analyze_image), media persistence, and prompt-attachment tests.

Covers: arun_pi_text's ``@<path>`` image args, the /api/vision/analyze
validation + happy path, media-route sanitization and 404s, the
turn-persistence media hook, attachment upload validation + manifest, and
chats.post_message's prepend-then-clear.
"""

from __future__ import annotations

import asyncio
import base64
import io
import json
from pathlib import Path

import pytest
from fastapi import HTTPException
from starlette.datastructures import UploadFile

from crack_server import attachments, chats, paths, pi_runner, render, routes_chats, routes_sub_agents, steprun, vision
from crack_server.routes_vision import api_vision_analyze
from crack_server.state import JsonState
from crack_server.steprun import TurnPersister
from tests.test_plan41 import fake_pi  # noqa: F401  (fixture)
from tests.test_wait_join import _json_request

PNG_BYTES = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
)

CHAT_ID = "1234567890123"


@pytest.fixture
def root(tmp_path, monkeypatch):
    monkeypatch.setenv("CRACK_PI_PROJECT_ROOT", str(tmp_path))
    return tmp_path


def _write_png(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(PNG_BYTES)
    return path


def _fake_describe(text: str = "a screenshot"):
    async def fake_analyze(prompt, image_paths):
        return text

    return fake_analyze


def _seed_manifest(state: JsonState, saved_path: str = "/abs/x.png") -> None:
    state.path.parent.mkdir(parents=True, exist_ok=True)
    state.write({"images": [{
        "id": "abcdef123456.png",
        "filename": "x.png",
        "saved_path": saved_path,
        "description": "shot",
        "uploaded_at": 0.0,
    }]})


# ---------------------------------------------------------------------------
# arun_pi_text: image args on the command line
# ---------------------------------------------------------------------------


def test_run_pi_text_image_args(fake_pi):
    fake_pi.set_script(["ok"])
    text, _ = pi_runner.run_pi_text(
        "describe it",
        log_prefix="test",
        model="moonshotai/x",
        image_paths=[Path("/tmp/a.png"), Path("/tmp/b.jpg")],
    )
    assert text == "text-response"
    argv = fake_pi.argv(1)
    assert argv[-1] == "describe it"  # prompt still last
    assert argv[-3:-1] == ["@/tmp/a.png", "@/tmp/b.jpg"]
    assert "--no-tools" in argv


def test_run_pi_text_no_image_args_unchanged(fake_pi):
    fake_pi.set_script(["ok"])
    pi_runner.run_pi_text("plain", log_prefix="test", model="moonshotai/x")
    argv = fake_pi.argv(1)
    assert not any(a.startswith("@") for a in argv)


# ---------------------------------------------------------------------------
# /api/vision/analyze
# ---------------------------------------------------------------------------


@pytest.mark.anyio
async def test_vision_analyze_rejects_missing_and_invalid(root):
    request = _json_request(
        {"prompt": "what is this", "image_paths": ["/nope/missing.png"]},
        "/api/vision/analyze",
    )
    with pytest.raises(HTTPException) as excinfo:
        await api_vision_analyze(request)
    assert excinfo.value.status_code == 400
    assert "not found: /nope/missing.png" in excinfo.value.detail

    not_img = root / "notes.txt"
    not_img.write_text("hello", encoding="utf-8")
    request = _json_request(
        {"prompt": "what is this", "image_paths": ["/nope/missing.png", str(not_img)]},
        "/api/vision/analyze",
    )
    with pytest.raises(HTTPException) as excinfo:
        await api_vision_analyze(request)
    detail = excinfo.value.detail
    assert "not found: /nope/missing.png" in detail
    assert f"not a valid image: {not_img}" in detail


@pytest.mark.anyio
async def test_vision_analyze_happy_path(root, monkeypatch):
    img = _write_png(root / "shot.png")
    monkeypatch.setattr(vision, "analyze", _fake_describe("a red dot"))
    request = _json_request(
        {"prompt": "what is this", "image_paths": [str(img)]},
        "/api/vision/analyze",
    )
    response = await api_vision_analyze(request)
    assert json.loads(response.body) == {"text": "a red dot"}


@pytest.mark.anyio
async def test_vision_analyze_resolves_relative_paths(root, monkeypatch):
    _write_png(root / "shot.png")
    monkeypatch.setattr(vision, "analyze", _fake_describe())
    request = _json_request(
        {"prompt": "what is this", "image_paths": ["shot.png"]},
        "/api/vision/analyze",
    )
    response = await api_vision_analyze(request)
    assert response.status_code == 200


# ---------------------------------------------------------------------------
# Media-serving routes: sanitization + 404s
# ---------------------------------------------------------------------------



def test_chat_media_route(root):
    paths.create_chat(CHAT_ID, "moonshotai/x")
    _write_png(paths.chat_media_dir(CHAT_ID) / "abc123.png")
    assert routes_chats.chat_media(CHAT_ID, "abc123.png").status_code == 200
    with pytest.raises(HTTPException) as excinfo:
        routes_chats.chat_media(CHAT_ID, "missing.png")
    assert excinfo.value.status_code == 404


def test_run_media_route(root):
    paths.create_chat(CHAT_ID, "moonshotai/x")
    run_id = "1234567890124_abcd1234"
    _write_png(paths.run_media_dir(CHAT_ID, run_id) / "abc123.png")
    assert routes_sub_agents.run_media(CHAT_ID, run_id, "abc123.png").status_code == 200
    with pytest.raises(HTTPException) as excinfo:
        routes_sub_agents.run_media(CHAT_ID, run_id, "missing.png")
    assert excinfo.value.status_code == 404
    with pytest.raises(HTTPException) as excinfo:
        routes_sub_agents.run_media(CHAT_ID, "not-a-run-id", "abc123.png")
    assert excinfo.value.status_code == 404


# ---------------------------------------------------------------------------
# Turn-persistence media hook
# ---------------------------------------------------------------------------


def test_persister_attaches_media_only_for_valid_images(root):
    img = _write_png(root / "shot.png")
    not_img = root / "notes.txt"
    not_img.write_text("x", encoding="utf-8")
    media_dir = root / "task_media"
    state = JsonState(root / "state.json")
    persister = TurnPersister(
        state, media_dir=media_dir, media_url_prefix="/tasks/t1/media"
    )
    persister.persist({
        "text": "",
        "thinking": "",
        "tool_blocks": [
            {"name": "read", "input": {"path": str(img)}, "output": ""},
            {"name": "read", "input": {"path": str(not_img)}, "output": ""},
            {"name": "read", "input": {"path": "/missing/x.png"}, "output": ""},
            {"name": "read", "input": {"path": str(root / "code.py")}, "output": ""},
            {"name": "analyze_image",
             "input": {"prompt": "?", "image_paths": [str(img), "/missing/y.png"]},
             "output": ""},
        ],
    }, 1)
    blocks = state.read()["turns"][0]["tool_blocks"]
    assert "media" not in blocks[1]  # not a valid image — skipped silently
    assert "media" not in blocks[2]  # missing — skipped silently
    assert "media" not in blocks[3]  # non-image extension — not a candidate
    for block, n_urls in ((blocks[0], 1), (blocks[4], 1)):
        assert len(block["media"]) == n_urls
        url = block["media"][0]["url"]
        assert url.startswith("/tasks/t1/media/")
        assert (media_dir / url.rsplit("/", 1)[-1]).is_file()


def test_persister_without_media_dir_leaves_blocks_alone(root):
    state = JsonState(root / "state.json")
    TurnPersister(state).persist(
        {"tool_blocks": [{"name": "read", "input": {"path": "/x.png"}}]}, 1
    )
    assert "media" not in state.read()["turns"][0]["tool_blocks"][0]


# ---------------------------------------------------------------------------
# Attachments: upload validation, manifest, weaving
# ---------------------------------------------------------------------------


def test_add_attachment_validates_and_describes(root, monkeypatch):
    monkeypatch.setattr(vision, "analyze", _fake_describe("a screenshot"))
    paths.create_chat(CHAT_ID, "moonshotai/x")
    state = paths.chat_attachments_state(CHAT_ID)
    directory = paths.chat_attachments_dir(CHAT_ID)
    entry = asyncio.run(attachments.add_attachment(state, directory, PNG_BYTES, "shot.png"))
    assert entry["description"] == "a screenshot"
    assert Path(entry["saved_path"]).is_file()
    assert attachments.list_attachments(state) == [entry]

    # Same bytes again → same entry, no duplicate.
    again = asyncio.run(attachments.add_attachment(state, directory, PNG_BYTES, "shot.png"))
    assert again["id"] == entry["id"]
    assert len(attachments.list_attachments(state)) == 1

    with pytest.raises(ValueError):
        asyncio.run(attachments.add_attachment(state, directory, b"not an image", "x.png"))


@pytest.mark.anyio
async def test_attachment_upload_route(root, monkeypatch):
    monkeypatch.setattr(vision, "analyze", _fake_describe("a diagram"))
    paths.create_chat(CHAT_ID, "moonshotai/x")

    def _upload(name: str, data: bytes) -> UploadFile:
        return UploadFile(io.BytesIO(data), filename=name)

    response = await routes_chats.api_chat_attachment_upload(
        CHAT_ID, file=_upload("shot.png", PNG_BYTES)
    )
    assert response.status_code == 200
    assert b"tool-thumb" in response.body
    entries = attachments.list_attachments(paths.chat_attachments_state(CHAT_ID))
    assert len(entries) == 1
    assert entries[0]["description"] == "a diagram"

    with pytest.raises(HTTPException) as excinfo:
        await routes_chats.api_chat_attachment_upload(CHAT_ID, file=_upload("x.png", b"junk"))
    assert excinfo.value.status_code == 400

    # Delete removes the manifest entry and the file.
    entry_id = entries[0]["id"]
    saved = Path(entries[0]["saved_path"])
    assert saved.is_file()
    routes_chats.api_chat_attachment_delete(CHAT_ID, entry_id)
    assert attachments.list_attachments(paths.chat_attachments_state(CHAT_ID)) == []
    assert not saved.exists()
    with pytest.raises(HTTPException) as excinfo:
        routes_chats.api_chat_attachment_delete(CHAT_ID, entry_id)
    assert excinfo.value.status_code == 404


def test_format_block_shape():
    entries = [
        {"saved_path": "/a/1.png", "description": "first"},
        {"saved_path": "/a/2.png", "description": "second"},
    ]
    block = attachments.format_block(entries)
    assert block == (
        "User attached 2 images:\n"
        "- /a/1.png\n"
        "  - first\n"
        "- /a/2.png\n"
        "  - second\n"
        "You may use the analyze_image tool to ask further questions about these images.\n"
        "----"
    )



def test_chat_post_message_weaves_then_clears(root):
    paths.create_chat(CHAT_ID, "moonshotai/x")
    state = paths.chat_attachments_state(CHAT_ID)
    _seed_manifest(state)

    chats.post_message(CHAT_ID, "hello there", None)
    pending = paths.chat_state(CHAT_ID).read()["pending"]
    assert len(pending) == 1
    assert pending[0]["user"].startswith("User attached 1 image:\n- /abs/x.png\n  - shot\n")
    assert pending[0]["user"].endswith("----\n\nhello there")
    # Manifest cleared so the next message is not re-woven.
    assert attachments.list_attachments(state) == []

    chats.post_message(CHAT_ID, "second", None)
    pending = paths.chat_state(CHAT_ID).read()["pending"]
    assert pending[1]["user"] == "second"
    assert "media" not in pending[1]


# ---------------------------------------------------------------------------
# Prompt-attachment media → recorded user_prompt entries → thumbnails
# ---------------------------------------------------------------------------


def test_chat_post_message_stashes_media_onto_the_exchange(root):
    paths.create_chat(CHAT_ID, "moonshotai/x")
    _seed_manifest(paths.chat_attachments_state(CHAT_ID))

    chats.post_message(CHAT_ID, "hello there", None)
    pending = paths.chat_state(CHAT_ID).read()["pending"]
    assert pending[0]["media"] == [{
        "url": f"/chats/{CHAT_ID}/attachments/abcdef123456.png",
        "src": "/abs/x.png",
        "description": "shot",
    }]

    # _pop_pending carries the media list onto the exchange for rendering.
    item = chats._pop_pending(CHAT_ID)
    assert item is not None
    exchange = paths.chat_state(CHAT_ID).read()["exchanges"][0]
    assert exchange["media"] == pending[0]["media"]


def test_render_exchanges_shows_prompt_thumbs_from_exchange_media():
    media = [{"url": "/chats/c1/attachments/x.png", "src": "/abs/x.png",
              "description": "shot"}]
    exchanges = [{"user": "look at this", "turns": [], "media": media}]
    msgs = render.render_exchanges(exchanges, render.render_turn_msgs)
    assert len(msgs) == 1
    assert 'class="tool-thumb"' in msgs[0]
    assert 'src="/chats/c1/attachments/x.png"' in msgs[0]


def test_render_user_prompt_msg_renders_media_thumbs():
    html = render.render_user_prompt_msg({
        "kind": "user_prompt",
        "compiled": "do it",
        "label": "chat",
        "media": [{"url": "/chats/c1/attachments/x.png", "src": "/abs/x.png",
                   "description": "shot"}],
    })
    assert 'class="tool-thumb"' in html
    assert 'src="/chats/c1/attachments/x.png"' in html
    assert 'title="/abs/x.png"' in html
    # No media → no thumb strip.
    assert "tool-thumb" not in render.render_user_prompt_msg(
        {"kind": "user_prompt", "compiled": "do it"}
    )


def test_prompt_recorder_attaches_media_list_and_callable(tmp_path):
    state = JsonState(tmp_path / "state.json")
    persister = TurnPersister(state)
    rows = [{"url": "/chats/c1/attachments/x.png", "src": "/a/x.png",
             "description": "shot"}]

    steprun.prompt_recorder(persister, "hop 1", "t.md", media=rows)(
        {"kind": "user_prompt", "compiled": "P1", "at": 1.0}
    )
    steprun.prompt_recorder(persister, "hop 2", "t.md", media=lambda: rows)(
        {"kind": "user_prompt", "compiled": "P2", "at": 2.0}
    )
    steprun.prompt_recorder(persister, "hop 3", "t.md")(
        {"kind": "user_prompt", "compiled": "P3", "at": 3.0}
    )
    turns = state.read()["turns"]
    assert turns[0]["media"] == rows
    assert turns[1]["media"] == rows
    assert "media" not in turns[2]


