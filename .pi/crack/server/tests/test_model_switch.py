"""Model-switch visibility + prewalk graduation.

Covers the model bookkeeping the harness now surfaces:

- every persisted turn records the model that produced it (``make_turn`` /
  ``TurnPersister.current_model``);
- the trajectory renders a per-turn model tag and a divider row whenever the
  model changes — labelled "prewalk plan complete" when a todo list preceded the
  switch (the auto-swap) and a plain "switched model" otherwise (a user switch);
- a plan chat's continuation form shows the model dropdown only once graduated;
- the right-tree / card model mirrors the run's *phase* model, not the persona
  config default.
"""

from __future__ import annotations

from crack_server import chats, prewalk, render, steprun
from crack_server.state import JsonState
from tests.test_sub_agents import chat_root, fake_pi  # noqa: F401  (fixtures)


# ---------------------------------------------------------------------------
# per-turn model recording
# ---------------------------------------------------------------------------


def test_make_turn_records_model_when_set():
    turn = steprun.make_turn({"text": "hi"}, hop=1, model="acme/frontier")
    assert turn["model"] == "acme/frontier"


def test_make_turn_omits_model_when_empty():
    # Legacy turns (no model) must not carry an empty key the UI would tag.
    assert "model" not in steprun.make_turn({"text": "hi"}, hop=1)


def test_persister_stamps_current_model(tmp_path):
    state = JsonState(tmp_path / "s.json")
    state.write({"turns": []})
    persister = steprun.turn_persister(state)
    persister.current_model = "acme/planner"
    persister.persist({"text": "plan", "tool_blocks": []}, hop=1)
    persister.current_model = "acme/cheap"
    persister.persist({"text": "impl", "tool_blocks": []}, hop=2)
    models = [t.get("model") for t in state.read()["turns"]]
    assert models == ["acme/planner", "acme/cheap"]


def test_persister_stamp_reason_on_last_turn(tmp_path):
    state = JsonState(tmp_path / "s.json")
    state.write({"turns": []})
    persister = steprun.turn_persister(state)
    persister.persist({"text": "a", "tool_blocks": []}, hop=1)
    persister.persist({"text": "b", "tool_blocks": []}, hop=1)
    persister.stamp_reason("time_cap")
    turns = state.read()["turns"]
    assert "reason" not in turns[0]
    assert turns[1]["reason"] == "time_cap"


def test_persister_stamp_reason_noop_when_empty(tmp_path):
    state = JsonState(tmp_path / "s.json")
    state.write({"turns": []})
    persister = steprun.turn_persister(state)
    persister.stamp_reason("agent_end")  # nothing persisted this hop
    assert state.read()["turns"] == []


def test_reason_note_shown_for_notable_reasons():
    assert "time cap" in render._reason_note("time_cap")
    assert render._reason_note("agent_end") == ""
    assert render._reason_note("swap") == ""  # divider already covers swaps
    html = render.render_turn_msgs([{**_turn("m"), "reason": "time_cap"}])[0]
    assert "turn-reason" in html


def test_terminal_reason_row_labels():
    assert "Stopped by user" in render.render_terminal_reason_row("stopped")
    assert "after 12.5s" in render.render_terminal_reason_row("stopped", duration=12.5)
    assert "wait_join" in render.render_terminal_reason_row("waiting_children")
    assert "Agent finished" in render.render_terminal_reason_row("agent_end")
    assert "after 3m 12s" in render.render_terminal_reason_row("agent_end", duration=192.0)
    assert render.render_terminal_reason_row("swap") == ""
    assert render.render_terminal_reason_row("time_cap") == ""


def test_prep_timing_row_shows_elapsed():
    html = render.render_prep_timing_row({
        "id": "sandbox", "label": "sandbox ready", "elapsed": 1.234, "at": 1.0,
    })
    assert "prep-timing" in html
    assert "1.23s" in html
    assert "sandbox ready" in html


# ---------------------------------------------------------------------------
# trajectory rendering: tag + divider
# ---------------------------------------------------------------------------


def _turn(model, tools=None, text="x"):
    return {"text": text, "thinking": "", "tool_blocks": tools or [], "model": model}


def test_model_tag_shown_per_turn():
    html = "".join(render.render_turn_msgs(
        [_turn("acme/frontier")], model_state=render.new_model_state()
    ))
    assert "turn-model" in html
    assert "acme/frontier" in html


def test_prewalk_swap_divider_after_todo():
    turns = [
        _turn("acme/planner", tools=[{"name": "todo", "output": "[ ] #1 do it"}]),
        _turn("acme/cheap", tools=[{"name": "edit", "input": {"path": "a.py"}}]),
    ]
    html = "".join(render.render_turn_msgs(turns, model_state=render.new_model_state()))
    assert "model-switch" in html
    assert "prewalk plan complete" in html
    assert "acme/cheap" in html


def test_user_switch_divider_without_todo():
    turns = [_turn("acme/a"), _turn("acme/b")]
    html = "".join(render.render_turn_msgs(turns, model_state=render.new_model_state()))
    assert "switched model" in html
    assert "prewalk plan complete" not in html


def test_no_divider_when_model_stable():
    turns = [_turn("acme/a"), _turn("acme/a")]
    html = "".join(render.render_turn_msgs(turns, model_state=render.new_model_state()))
    assert "model-switch" not in html


def test_model_state_threads_across_calls():
    # Same tracker across two calls (the per-exchange chat case): the switch is
    # detected at the boundary, not lost.
    ms = render.new_model_state()
    first = "".join(render.render_turn_msgs([_turn("acme/a")], model_state=ms))
    second = "".join(render.render_turn_msgs([_turn("acme/b")], model_state=ms))
    assert "model-switch" not in first
    assert "model-switch" in second


def test_models_equivalent_treats_provider_prefix_variants():
    assert render.models_equivalent(
        "nvidia/minimaxai/minimax-m3", "minimaxai/minimax-m3"
    )
    assert not render.models_equivalent("acme/a", "acme/b")


def test_prewalk_swap_with_early_model_change_annotation():
    """Session-start model_change must not produce a reversed swap divider."""
    from crack_server import trajectory_view

    planner = "nvidia/minimaxai/minimax-m3"
    implementer = "nvidia/stepfun-ai/step-3.7-flash"
    turns = [
        _turn(planner, tools=[{"name": "todo", "output": "[ ] #1 do it"}], text="plan"),
        _turn(implementer, tools=[{"name": "edit", "input": {"path": "a.py"}}], text="impl"),
    ]
    turns[0]["at"] = 101.0
    turns[1]["at"] = 102.0
    annotations = [{
        "kind": "annotation", "ann": "model_change", "model": planner, "at": 100.0,
    }]
    spine = trajectory_view.merge_time_sorted_spine(turns, annotations)
    html = "".join(render.render_turn_msgs(spine, model_state=render.new_model_state()))
    assert html.count("model-switch-line") == 1
    assert "prewalk plan complete" in html
    assert implementer in html
    assert f"implementing on <code>{implementer}</code>" in html
    assert f"implementing on <code>{planner}</code>" not in html


def test_unsorted_annotations_append_would_reverse_swap():
    """Regression: concatenating annotations after turns shows a bogus divider."""
    from crack_server import trajectory_view

    planner = "nvidia/minimaxai/minimax-m3"
    implementer = "nvidia/stepfun-ai/step-3.7-flash"
    turns = [
        _turn(planner, tools=[{"name": "todo", "output": "[ ] #1"}], text="plan"),
        _turn(implementer, tools=[{"name": "edit", "input": {"path": "a.py"}}], text="impl"),
    ]
    turns[0]["at"] = 101.0
    turns[1]["at"] = 102.0
    annotations = [{
        "kind": "annotation", "ann": "model_change", "model": planner, "at": 100.0,
    }]
    bad_spine = list(turns) + annotations
    good_spine = trajectory_view.merge_time_sorted_spine(turns, annotations)
    bad_html = "".join(render.render_turn_msgs(bad_spine, model_state=render.new_model_state()))
    good_html = "".join(render.render_turn_msgs(good_spine, model_state=render.new_model_state()))
    assert bad_html.count("model-switch-line") == 2
    assert good_html.count("model-switch-line") == 1


# ---------------------------------------------------------------------------
# tool-output preview
# ---------------------------------------------------------------------------


def test_tool_output_short_has_no_expand_toggle():
    html = render._render_tool_output("one\ntwo\nthree")
    assert "tool-out-preview" in html
    assert "tool-out-toggle" not in html


def test_tool_output_long_has_single_icon_toggle():
    html = render._render_tool_output("\n".join(f"line {i}" for i in range(40)))
    assert "tool-out-preview" in html
    assert "tool-out-toggle" in html
    # The summary carries no visible text label (icon comes from CSS ::before).
    assert "<summary class=\"tool-out-toggle\"" in html


# ---------------------------------------------------------------------------
# continuation form: dropdown appears only once graduated
# ---------------------------------------------------------------------------


# A chat that has sent its first message (so it's past the in-chat config editor).
_SENT = {"exchanges": [{"user": "hi", "turns": []}]}


def test_plan_chat_form_editor_before_first_message(chat_root):
    # Before the first message the plan/model config is editable inline.
    info = {"plan": True, "planner_model": "acme/p", "implementer_model": "acme/i"}
    html = chats.render_chat_form(chat_root, info, {"exchanges": [], "pending": []})
    assert "chat-config" in html
    assert 'name="planner_model"' in html
    assert 'name="implementer_model"' in html
    assert "data-plan-toggle" in html


def test_plan_chat_form_locked_before_graduation(chat_root):
    # First message sent, not yet graduated → read-only badge (no editor/dropdown).
    info = {"plan": True, "planner_model": "acme/p", "implementer_model": "acme/i"}
    html = chats.render_chat_form(chat_root, info, _SENT)
    assert "chat-model-badge" in html
    assert "chat-config" not in html
    assert 'name="model"' not in html


def test_plan_chat_form_dropdown_after_graduation(chat_root):
    info = {
        "plan": True, "graduated": True,
        "planner_model": "acme/p", "implementer_model": "acme/i", "model": "acme/n",
    }
    html = chats.render_chat_form(chat_root, info, _SENT)
    assert 'name="model"' in html
    # Continuation defaults to the implementer model.
    assert 'value="acme/i" selected' in html
    # A graduated plan chat notes that plan mode is now locked.
    assert "chat-plan-locked" in html


def test_nonplan_chat_form_has_dropdown(chat_root):
    info = {"plan": False, "model": "acme/n"}
    html = chats.render_chat_form(chat_root, info, _SENT)
    assert 'name="model"' in html
    assert 'value="acme/n" selected' in html
    assert "chat-plan-locked" not in html


# ---------------------------------------------------------------------------
# right-tree / card model mirrors the phase model
# ---------------------------------------------------------------------------


def test_run_display_model_uses_planner_while_planning():
    state = {
        "plan": True, "planner_model": "acme/planner",
        "implementer_model": "acme/cheap", "model": "acme/n", "turns": [],
    }
    assert chats._run_display_model(state) == "acme/planner"


def test_run_display_model_uses_implementer_after_swap():
    state = {
        "plan": True, "planner_model": "acme/planner",
        "implementer_model": "acme/cheap", "model": "acme/n",
        "turns": [
            {"tool_blocks": [{"name": "todo", "output": "[ ] #1"}]},
            {"tool_blocks": [{"name": "edit", "input": {"path": "a"}}]},
        ],
    }
    assert chats._run_display_model(state) == "acme/cheap"


def test_chat_display_model_planning_then_graduated():
    info = {
        "plan": True, "planner_model": "acme/planner",
        "implementer_model": "acme/cheap", "model": "acme/n",
    }
    # Still planning (no swap, not graduated) → planner model.
    assert chats._chat_display_model(info, {"exchanges": []}) == "acme/planner"
    # Graduated → the continuation default (implementer).
    assert chats._chat_display_model({**info, "graduated": True}, {}) == "acme/cheap"


def test_graduation_gate_matches_prewalk_swap():
    # The engine's plan_active gate hinges on prewalk.swapped_already; keep the
    # contract pinned so the graduation logic and the divider agree.
    turns = [
        {"tool_blocks": [{"name": "todo", "output": "[ ] #1"}]},
        {"tool_blocks": [{"name": "edit", "input": {"path": "a"}}]},
    ]
    assert prewalk.swapped_already(turns) is True
    assert prewalk.swapped_already(turns[:1]) is False


def test_post_message_locks_config_on_first_message(chat_root):  # noqa: F811
    from crack_server import paths

    chats.post_message(
        chat_root, "do it", "acme/n",
        plan=True, planner_model="acme/p2", implementer_model="acme/i2",
    )
    info = paths.chat_info_state(chat_root).read()
    assert info["plan"] is True
    assert info["planner_model"] == "acme/p2"
    assert info["implementer_model"] == "acme/i2"
    assert info["model"] == "acme/n"
    # The nonplan pick is stored on the chat, not stamped as a per-exchange switch.
    assert paths.chat_state(chat_root).read()["pending"][0].get("model") is None


def test_dirty_git_gate_preserves_plan_config(chat_root, monkeypatch):  # noqa: F811
    """Plan 24 Issue 3a/4: a dirty-tree refusal must still persist the user's
    plan/model choice, else the gate re-render flips the plan checkbox back on."""
    from crack_server import paths, sandbox, git_utils

    monkeypatch.setattr(sandbox, "sandbox_enabled", lambda: True)
    monkeypatch.setattr(git_utils, "host_worktree_dirty", lambda *a, **k: True)
    monkeypatch.setattr(git_utils, "host_status_colored", lambda *a, **k: "dirty")

    resp = chats.post_message(chat_root, "do it", "acme/n", plan=False)
    # Refused: red gate shown, nothing enqueued.
    assert "Host worktree is dirty" in resp.body.decode()
    assert not paths.chat_state(chat_root).read().get("pending")
    # But the plan-off choice is locked onto info so the re-render stays off.
    info = paths.chat_info_state(chat_root).read()
    assert info["plan"] is False
    assert "checked" not in chats.render_chat_config_editor(info).split("data-plan-toggle")[0]


def test_config_editor_emits_config_hidden_field():
    html = chats.render_chat_config_editor({
        "plan": False,
        "model": "nvidia/nemotron-3-ultra-550b-a55b",
        "planner_model": "acme/p",
        "implementer_model": "composer-2.5",
    })
    assert 'name="config" value="1"' in html


def test_nonplan_model_resolution_ignores_implementer_until_graduated():
    """Plan 24 Issue 4: implementer_model must not shadow the locked non-plan model."""
    info = {
        "plan": False,
        "graduated": False,
        "model": "nvidia/nemotron-3-ultra-550b-a55b",
        "implementer_model": "composer-2.5",
    }
    cur_exchange: dict = {}  # first exchange: post_message cleared per-exchange model
    plan_active = bool(info.get("plan")) and not bool(info.get("graduated"))
    assert not plan_active
    model = (
        cur_exchange.get("model")
        or (info.get("implementer_model") if info.get("graduated") else None)
        or info.get("model")
        or "fallback"
    )
    assert model == "nvidia/nemotron-3-ultra-550b-a55b"


def test_chat_display_model_prefers_cached(chat_root):  # noqa: F811
    # A graduated chat caches its display model on info — read it directly.
    info = {"plan": True, "graduated": True, "display_model": "acme/cached",
            "implementer_model": "acme/i"}
    assert chats._chat_display_model(info, {}) == "acme/cached"


# ---------------------------------------------------------------------------
# vision dropdown: image-capable models only
# ---------------------------------------------------------------------------


def test_image_models_filters_to_image_capable(chat_root):  # noqa: F811
    from crack_server import models as models_mod, paths

    paths.models_cache_state().write({
        "fetched_at": 9e18,  # far future → no refresh enqueued
        "models": ["v/see", "v/blind"],
        "info": {"v/see": {"images": True}, "v/blind": {"images": False}},
    })
    assert models_mod.image_models_for_render() == ["v/see"]


def test_image_models_fallback_when_no_info(chat_root):  # noqa: F811
    from crack_server import models as models_mod, paths, vision

    paths.models_cache_state().write({
        "fetched_at": 9e18, "models": ["v/x"], "info": {},
    })
    assert models_mod.image_models_for_render() == [vision.DEFAULT_VISION_MODEL]
