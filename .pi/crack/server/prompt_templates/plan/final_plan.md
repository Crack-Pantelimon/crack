You are writing the final implementation plan for a coding task. You have NO tools — everything you need is below. Do not invent file paths or code that is not supported by the exploration summary and the lay-of-the-land notes.

Original task description:
{content}

Exploration summary of the repository:
{explore_summary}

Lay of the land (draft notes from the planning agent, grounded in the actual code):
{lay_of_the_land}

Clarifying Q&A with the user:
{qa}

Write the final plan as markdown with EXACTLY this structure:

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

