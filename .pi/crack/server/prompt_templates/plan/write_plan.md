You now have everything you need. Write the final implementation plan as a file on disk.

Write the plan to this exact path:
{plan_path}

Use the `write` tool to create (or overwrite) the file, and the `edit` tool to revise sections as you refine it. You may keep using `bash`/`read` to double-check code details while writing. Take as many turns as you need — the plan file on disk is the deliverable, not your chat messages.

For reference, the task and what you have established so far:

Original task description:
{content}

Exploration summary of the repository:
{explore_summary}

Draft plan (your notes from the planning step, grounded in the actual code):
{draft_plan}

Clarifying Q&A with the user:
{qa}

The plan file MUST be markdown with EXACTLY this structure (these headings are verified mechanically — the step fails if any is missing):

# Plan

## Initial build/check instructions
How to build the project and run its existing checks before changing anything (exact commands), so regressions can be detected.

## Problem statement
What the task is and why, in a few paragraphs, grounded in the code as it exists today.

## Changes
For EVERY code path that must change: a subsection naming the file (and lines where known), a code sample of the change (before/after or sketch), and the motivation. Cover the full set of changes, not just the first one.

## What NOT to change
An explicit list of files/behaviors/interfaces that must stay untouched, so the implementation does not drift.

## Automatic verification
Commands and test steps that can be run non-interactively to prove the change works (build, tests, linters), in order.

## Manual verification
Step-by-step human checks (UI flows, outputs to eyeball) for anything automation cannot cover.

## Overview / Summary
A short recap: the goal, the shape of the solution, and the main risks.

Do not invent file paths or code that is not supported by the exploration and your notes. Do not modify any file other than the plan file at {plan_path}.

When the plan file is complete on disk, reply with a short summary of the plan and make no further tool calls.
