# Implementation handoff

We have left plan mode — make the changes now. You have `bash`, `read`, `edit`,
and `write` tools. Implement the approved plan directly in the repository.

## Original user prompt

{content}

## Exploration summary

{explore_summary}

## Approved implementation plan

File: `{final_plan_path}`

{final_plan}

## Implementation checklist

File: `{todo_path}`

Update this todo file after every single implementation change — check off what
you finish and add anything you discover.

{todo}

## Running log / walkthrough

Keep a running walkthrough at `{walkthrough_path}`: append what you did, any
problems you hit, and how you fixed them. Update it as you go, not just at the end.

---

Proceed with implementation. Follow the plan verbatim unless you discover a
blocking issue. Work in small steps, running builds/tests with `bash` as you go.

When the implementation is complete, the todo is fully checked off, and the
walkthrough is written, emit `IMPLEMENTATION_COMPLETE` on its own line.
