"""Stage s04: Implementation — static handoff prompt after plan approval (no AI)."""

from __future__ import annotations

from crack_server import paths
from crack_server.stages.base import Stage
from crack_server import app as _ui


def _esc(text: str) -> str:
    return _ui._esc(text)


class S04Implementation(Stage):
    slug = "implementation"
    name = "Implementation"
    parts = []

    def status(self, task_id: str) -> str:
        from crack_server import stages

        review = stages.get("plan_review")
        if review is not None and review.status(task_id) == "done":
            return "awaiting"
        return "disabled"

    def is_enabled(self, task_id: str) -> bool:
        from crack_server import stages

        review = stages.get("plan_review")
        return review is not None and review.status(task_id) == "done"

    def start(self, task_id: str) -> None:
        pass

    def _assemble_handoff(self, task_id: str) -> str:
        final_plan_path = paths.plan_dir(task_id) / "final_plan.md"
        todo_path = paths.plan_todo_path(task_id)
        try:
            final_plan = paths.read_plan_artefact(task_id, "final_plan.md")
        except FileNotFoundError:
            final_plan = "(no final plan)"
        try:
            todo = paths.read_plan_artefact(task_id, "todo.md")
        except FileNotFoundError:
            todo = "(no todo checklist)"
        explore_summary = paths.read_explore_state(task_id).get("summary_md", "(none)")
        content = paths.read_all_prompts_joined(task_id)

        return (
            self.load_template("handoff.md")
            .replace("{content}", content or "(no prompts)")
            .replace("{explore_summary}", explore_summary)
            .replace("{final_plan}", final_plan)
            .replace("{final_plan_path}", str(final_plan_path))
            .replace("{todo}", todo)
            .replace("{todo_path}", str(todo_path))
        )

    def render_status(self, task_id: str) -> str:
        content_id = self.stage_content_id()
        parts: list[str] = []

        if not self.is_enabled(task_id):
            parts.append(
                '<div class="stage-msg"><p style="color: #888;">'
                "Approve the plan first to unlock implementation.</p></div>"
            )
            msg_count = 1
        else:
            handoff = self._assemble_handoff(task_id)
            parts.append(
                f'<div class="stage-msg implementation-handoff">'
                f"{_ui._render_markdown(handoff)}</div>"
            )
            parts.append(
                f'<div class="stage-msg"><label>Copy raw handoff prompt'
                f'<textarea readonly rows="16" class="handoff-raw">{_esc(handoff)}</textarea>'
                f"</label></div>"
            )
            msg_count = 2

        return self.wrap_status(
            task_id,
            "".join(parts),
            msg_count=msg_count,
            polling=False,
            extra_class="implementation-content",
        )


STAGE = S04Implementation()
