# Implementation review

The implementation phase is finished. Your job is to critically review the work
against the approved plan, then fix anything that is wrong. You have `bash`,
`read`, `edit`, and `write` tools — use them freely.

## Original user prompt

{content}

## Exploration summary

{explore_summary}

## Approved plan

File: `{final_plan_path}`

{final_plan}

## Implementation checklist

File: `{todo_path}`

{todo}

## Implementation walkthrough so far

File: `{walkthrough_path}`

{walkthrough}

---

## What to do

1. Run `git diff` (and `git status`) to see exactly what changed.
2. **Validate everything**: build the project, run the tests, run any demos or
   entry points the plan implies. Actually execute them with `bash` — do not
   assume they pass.
3. Review the diff critically against the plan and the original prompt. Be
   skeptical: look for missed requirements, half-done work, broken edge cases,
   and sloppy code.
4. **Fix wrong code directly.** You are allowed and expected to edit/write files
   to correct mistakes, always staying within the intent of the approved plan.
5. Keep updating the walkthrough at `{walkthrough_path}` (what you verified, what
   you fixed) and the todo at `{todo_path}`.
6. **Loop until clean**: if there are any compiler warnings, linter errors, or
   test failures, fix them and re-run. Do not stop while anything is failing.

When the build is green, the tests pass, there are no warnings, and the work
faithfully implements the plan, emit `REVIEW_COMPLETE` on its own line.
