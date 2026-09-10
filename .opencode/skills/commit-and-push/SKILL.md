---
name: commit-and-push
description: Group all pending working-tree changes into logically coherent commits with fitting messages, then push them to main. Use when the user asks to "commit and push", "commit everything", or wants uncommitted changes tidily committed and pushed without manual staging.
---

# Commit and Push

Group all currently uncommitted changes into one or more logically coherent
commits — each with a commit message that accurately describes that group —
and push the result to `main`.

## Workflow

1. **Inspect the working tree.**
   - Run `git status --porcelain=v1` to list all changed, added, deleted, and
     untracked files.
   - Run `git diff` (unstaged) and `git diff --staged` (already staged, if
     any) to understand the actual content of the changes.
   - Run `git log --oneline -10` to match the repo's existing commit message
     style (tense, language, prefix conventions like `feat:`, `fix:`, etc.).

2. **Check the current branch.**
   - Run `git branch --show-current`. If it is not `main`, ask the user
     whether to switch to `main` first or continue on the current branch —
     do not silently switch branches.
   - Run `git pull --ff-only` (or the repo's equivalent) before committing if
     the local `main` may be behind the remote, to avoid a diverging push.

3. **Group changes logically.**
   - Do not just run `git add -A` and make one giant commit unless the
     changes are genuinely a single cohesive unit of work.
   - Group by feature/concern/module, e.g.:
     - Source changes for one feature → one commit
     - Formatting-only / lint-fix changes → separate commit
     - Documentation/spec updates → separate commit
     - Generated/lock files → bundled with the change that caused them
     - Unrelated fixes discovered along the way → their own commit
   - Use `git add <specific files>` (or `git add -p` for partial staging
     within a file) to build each group precisely. Never stage files you
     have not reviewed.
   - Untracked files listed under `git status` must be added to git
     tracking (`git add <file>`) as part of the group they logically belong
     to — unless they are matched by `.gitignore` (check with `git
     check-ignore -v <file>` if unsure), in which case leave them untracked.
     Never leave new, non-ignored files untracked after the run.

4. **Follow repository quality gates before committing**, if defined in
   `AGENTS.md` or similar project instructions (e.g. linter, tests,
   formatting checks for this repo: `cargo clippy --all-targets -- -D
   warnings`, `cargo test`, `cargo fmt --check`). Run them once, on the full
   working tree, before starting to commit — commit only if they pass, or
   explain to the user why they don't and how you're proceeding.

5. **Commit each group** with `git commit -m "<message>"`.
   - Write commit messages in the imperative mood, matching the existing
     style found in step 1 (language, prefixes, punctuation).
   - Keep the summary line concise (~50-72 chars) and add a body only if the
     change needs more explanation than the summary allows.
   - Never invent scope you didn't actually verify — read the diff.

6. **Verify before pushing.**
   - Run `git status` again to confirm the working tree is clean (or that
     any remaining untracked files are intentionally excluded, e.g. via
     `.gitignore`).
   - Run `git log --oneline -n <number of new commits>` and show the user a
     quick summary of what will be pushed.

7. **Push.**
   - Run `git push` (or `git push -u origin main` if no upstream is set).
   - Report the pushed commits and the remote branch to the user.

## Rules

- Never force-push, rebase, or rewrite existing history.
- Never commit secrets, credentials, or files that look like local
  environment/config artifacts — flag them to the user instead.
- If quality gates (lint/tests/format) fail, stop and report the failure
  instead of committing broken code, unless the user explicitly says to
  commit anyway.
- If the working tree is already clean, report that there is nothing to
  commit instead of pushing a no-op.
- If changes are too tangled to separate safely (e.g. a single file mixes
  unrelated concerns line-by-line), say so and propose the best available
  grouping rather than silently making one commit.
