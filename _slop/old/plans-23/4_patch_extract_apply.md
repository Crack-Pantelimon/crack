# Plan 4 — Baseline-diff patch extraction, 95 MB guard, and auto-apply

> Read `0_overview.md`; requires Plans 1–3. This plan captures each sandboxed conversation's
> code delta as a patch and brings it back: sub-agent patches auto-apply into the parent
> overlay; top-level chat patches apply to the real host tree.

## Core mechanism (verified live — do not use a naive `git diff`)

The `:O` overlay **inherits the host repo's already-dirty index and worktree**, so a plain
`git add . && git diff` returns ~30 pre-existing files that were dirty before the run. Use a
baseline snapshot:

```bash
# at sandbox START (once, right after ensure_sandbox), inside the sandbox:
git -C /workspace add -A
BASE=$(git -C /workspace write-tree)        # snapshot of the dirty starting state
# ... agent works ...
# at conversation END (before destroy_sandbox), inside the sandbox:
git -C /workspace add -A
END=$(git -C /workspace write-tree)
git -C /workspace diff "$BASE" "$END" > <patch>   # EXACTLY this agent's delta
```

Store `BASE` in the run's shared dir at start:
`/crack-harness-data/unscripted_chats/<chat>/.../base_tree` (or run dir). Produce the patch
into the same dir as `patch.diff`.

## The 95 MB file guard (before finalizing the patch)

New untracked large files (build artifacts, `.blend`, screenshots) can bloat a patch.
`git add -A` respects `.gitignore`, so only **new, not-yet-ignored** files are at risk.
Algorithm (max 5 attempts), run inside the sandbox:

1. `git add -A` (scope note below), then list staged files whose blob size > 95 MB
   (`git diff --cached --numstat` won't give bytes; use
   `git diff --cached --name-only -z | xargs -0 -I{} sh -c 'test -f "{}" && wc -c < "{}"'`
   or check each path). 
2. If any are too big: `git reset` (unstage all), then **nag the agent** — enqueue a message
   to that conversation listing the offending **full paths** and asking it to `.gitignore`
   them or delete them, then retry. (For a sub-agent, the nag is a resume turn; for a chat,
   a system message.)
3. Retry up to 5 times. If still too big after 5, **stage everything except the big files**
   (`git add -A` then `git reset -- <big>...`) and produce that patch. 
4. **If the diff is empty, do NOT nag** — a read-only/exploration task legitimately produced
   no changes. Just record "no changes" and skip apply.

If a stop was **forceful** (kill), skip the retry loop entirely: stage all except any file
> 95 MB in one pass and emit whatever patch results.

**Scope:** the patch is the git delta of `/workspace` only. `/crack-harness-data` is a
separate mount and never appears in the git tree, so harness state can't leak (Plan 1 win).
Do NOT try to also exclude `.pi/crack/sub_agents` — persona edits are legitimate code changes
that should come back.

## Debug copy

The overlay upper already persists to `/crack-harness-data/overlays/<id>/upper` (Plan 2), so
a crashed container's changed files are inspectable there. Also keep `patch.diff` on the
volume for post-mortem. No extra work needed beyond writing the patch file.

## Auto-apply (LOCKED behavior)

When a conversation finishes and has a non-empty patch:

- **Sub-agent → parent:** in `sub_agents/runner.py:finish()` (or a new step right before the
  parent is resumed), apply the child patch into the **parent's overlay**:
  `podman exec <parent_sbx> git -C /workspace apply --3way <child_patch>`.
  Apply children in **finish order**. On success, continue as today (inbox the report).
- **Top-level chat → host:** apply to the real repo. Since crack-dev's `/workspace` is the
  real host tree, apply there (this is the one place the host tree is mutated, and only after
  the sandbox is done). NOTE this triggers crack-dev's auto-reload if server code changed —
  Plan 7 adds the health-check/rollback guard; until then, apply is fine for non-server edits.

### On conflict/failure (LOCKED)

Do **not** roll back. Leave git in the conflicted state (the failed `git apply --3way`
leaves conflict markers / a dirty index) and **tell the managing agent**: enqueue a message
to the parent (or chat) containing:
- the exact failure (`git apply` stderr),
- the **full path to the child patch** (`/crack-harness-data/.../patch.diff`),
- instructions: "Resolve the conflict directly in the working tree, finish applying the
  patch, then continue your task. The full patch is at <path> for reference."

The parent agent then uses its normal (now-unfrozen, since the child finished) edit/bash
tools to fix it. Because the child has finished, the freeze rule (Plan 6) no longer blocks
the parent's edits.

## Verification

1. **Delta correctness:** nemotron chat that edits ONE tracked file. Confirm `patch.diff`
   contains exactly that one file (not the pre-existing dirty set):
   ```bash
   docker exec crack-dev bash -exc 'grep -c "^diff --git" /crack-harness-data/unscripted_chats/<id>/.../patch.diff'
   ```
   → `1`.
2. **Top-level apply to host:** after that chat finishes, the edit **is** present in the real
   `/workspace` file (auto-applied), and `git -C /workspace diff` shows it.
3. **No-change task:** nemotron chat "run `git status` and summarize, then stop" → empty
   patch, **no nag message** in the transcript, chat idles cleanly.
4. **Big-file guard:** task "create `/workspace/big.bin` of 120MB with
   `head -c 120000000 /dev/zero > /workspace/big.bin`, then stop." Confirm the nag fires
   listing `big.bin`'s full path, and after 5 attempts the final patch excludes it. (Use a
   sub-agent or a chat; verify the nag text in the transcript.)
5. **Sub-agent auto-apply + conflict:** spawn a sub-agent that edits file X; confirm X's
   change lands in the parent overlay after wait_join. Then force a conflict (parent and
   child edit the same line — you can script this) and confirm the parent receives the
   conflict message with the patch path and that git is left conflicted (not rolled back).

## Gotchas

- `git write-tree` requires the index to reflect the worktree — always `git add -A` first.
  It does **not** create a commit, so it's cheap and side-effect-free.
- `git apply --3way` needs the blobs it references; they exist because BASE/END trees share
  the same object store within the conversation. Cross-conversation apply (child→parent) uses
  a plain unified diff — `--3way` still works if the parent tree has the base blobs; if not,
  fall back to `git apply --reject` and treat rejects as the conflict case.
- Escape/quote patch paths; conversation dirs contain no spaces but be defensive.
- Size check must use **bytes**, not `ls -h`. 95 MB = 95 * 1024 * 1024 if you mean MiB; the
  original spec said 95.0 MB — pick MB (10^6) and state which in your report.

## Report

`_slop/report-23/4_patch_extract_apply.md`: the extraction/apply code path, the exact
byte-threshold you used, all five verification results (with chat ids + patch paths), and the
conflict-message wording you shipped so Plan 7 / reviewers can see the managing-agent UX.
