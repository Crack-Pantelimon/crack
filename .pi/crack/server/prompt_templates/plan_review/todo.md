Generate an implementation checklist from this plan. Output ONLY markdown checklist items — no prose, no headings.

Format: `- [ ] item` for pending tasks, `- [x] item` only if already done (usually all unchecked).

Derive items from the plan's Changes, verification, and manual-check sections. One actionable item per line.

Completely skip (do not write) any tasks marked under "Manual Verification" subtitle. We only care about the other parts of the document, so skip any manual verification tasks completely from the todo list.

Plan:
{plan}
