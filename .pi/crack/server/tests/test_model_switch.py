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


def test_plan_chat_form_locked_before_graduation(chat_root):
    info = {"plan": True, "planner_model": "acme/p", "implementer_model": "acme/i"}
    html = chats.render_chat_form(chat_root, info)
    assert "chat-model-badge" in html
    assert 'name="model"' not in html


def test_plan_chat_form_dropdown_after_graduation(chat_root):
    info = {
        "plan": True, "graduated": True,
        "planner_model": "acme/p", "implementer_model": "acme/i", "model": "acme/n",
    }
    html = chats.render_chat_form(chat_root, info)
    assert 'name="model"' in html
    # Continuation defaults to the implementer model.
    assert 'value="acme/i" selected' in html


def test_nonplan_chat_form_has_dropdown(chat_root):
    info = {"plan": False, "model": "acme/n"}
    html = chats.render_chat_form(chat_root, info)
    assert 'name="model"' in html
    assert 'value="acme/n" selected' in html


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
