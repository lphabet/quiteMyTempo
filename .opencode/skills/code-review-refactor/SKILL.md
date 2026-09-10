---
name: code-review-refactor
description: Review the codebase for simplification opportunities, duplicated code, dead code/unused dependencies, unsafe error handling (unwrap/expect/panic), naming/module-structure consistency, and best-practice violations, then apply refactorings where sensible. Use when the user asks to "review the code", "check for duplicates/simplification", "refactor check", or wants a general code-quality pass over the project.
---

# Code Review & Refactor

Systematically review the codebase for quality issues and apply refactorings
where they are clearly beneficial and low-risk. This skill both **finds**
issues and **fixes** them (not merely a report).

## Workflow

1. **Establish a clean baseline.**
   - Run `git status --porcelain=v1`. If there are uncommitted changes, tell
     the user and ask whether to proceed anyway — refactoring on top of a
     dirty tree makes it hard to isolate regressions.
   - Run the project's quality gates once up front (per `AGENTS.md`, e.g.
     `cargo clippy --all-targets -- -D warnings`, `cargo test`,
     `cargo fmt --check`) to confirm the starting point is green. If it is
     not green, report that first — do not blame later changes on
     pre-existing failures.

2. **Survey the codebase structure.**
   - Get an overview of modules/files (e.g. `Cargo.toml` workspace layout,
     `src/` tree) to know what's in scope. For larger codebases, delegate
     exploration to the `explore` subagent (medium/thorough) instead of
     manually grepping everything.

3. **Review for the following issue categories.** For each finding, note the
   file/line, why it's an issue, and the proposed fix.

   - **Simplification opportunities**: overly complex control flow, unneeded
     abstractions/indirection, verbose code that has a more idiomatic
     equivalent, functions doing too much (candidates for splitting).
   - **Duplicated code**: repeated logic across files/modules that should be
     extracted into a shared function/trait/module. Use `grep`/`Task(explore)`
     to find structurally similar blocks, not just literal text matches.
   - **Dead code & unused dependencies**: unused functions/types/imports
     (`cargo clippy` catches much of this), and dependencies in `Cargo.toml`
     that are no longer used (check with `cargo machete` if available, else
     grep for crate usage manually).
   - **Error handling**: `unwrap()`, `expect()`, `panic!()`, `unreachable!()`
     in non-test, non-prototype code paths that should instead propagate a
     `Result`/`Option` or be justified with a comment.
   - **Naming & module structure**: inconsistent naming conventions, modules
     that don't match the existing project structure/conventions described in
     `specs/architecture.md` (if present).
   - **Best practices**: idiomatic Rust patterns (e.g. `impl Trait` vs boxed
     trait objects where appropriate, avoiding needless `clone()`, proper use
     of iterators vs manual loops), matching whatever `clippy` already
     enforces plus anything clippy doesn't catch.
   - **Test coverage gaps**: critical logic paths (state transitions, input
     parsing, error paths) with no corresponding test. Report these; do not
     silently invent tests unless the user asks for that separately — the
     focus of this skill is refactoring, not test authoring.
   - **Documentation drift**: if code behavior clearly diverges from
     `specs/*.md`, flag it but do not fix docs here — point the user at the
     `sync-docu` skill instead, to avoid overlapping responsibilities.

4. **Prioritize and confirm scope for risky changes.**
   - Low-risk, mechanical changes (removing dead code/unused deps, extracting
     obvious duplication, replacing an `unwrap()` with proper error
     propagation, applying `cargo clippy --fix` suggestions) can be applied
     directly.
   - For anything that changes public APIs, module boundaries, or behavior in
     a non-obvious way, summarize the proposed change and ask the user before
     applying it.

5. **Apply refactorings incrementally.**
   - Make one logical change at a time; avoid a single sprawling diff that
     mixes unrelated fixes.
   - After each meaningful change (or small batch of related changes), re-run
     the quality gates from step 1 to catch regressions early rather than at
     the very end.

6. **Final verification.**
   - Run all three quality gates one last time on the full result:
     `cargo clippy --all-targets -- -D warnings`, `cargo test`,
     `cargo fmt --check`.
   - Run `git diff --stat` and review the full `git diff` yourself to confirm
     no unintended behavior changes slipped in.
   - Summarize for the user: what was found, what was fixed, what was
     flagged but intentionally left alone (and why), and any new/untracked
     files that still need `git add`.

7. **Commit.**
   - Stage newly created files (`git add <file>`) per `AGENTS.md`.
   - Group the refactoring into logically coherent commits (reuse the
     grouping approach from the `commit-and-push` skill) rather than one
     giant commit, unless the user asks otherwise.
   - Do not push unless the user explicitly asks for it — this skill's job
     ends at reviewing/refactoring/committing locally.

## Rules

- Never disable or weaken a lint (`#[allow(...)]`, relaxing clippy config)
  just to make a finding go away — fix the underlying issue, or leave it and
  report it if fixing is out of scope.
- Never change behavior silently while "simplifying" — if a simplification
  would alter observable behavior, call it out explicitly and confirm with
  the user first.
- Do not commit if the quality gates are failing, unless the user explicitly
  says to commit anyway.
- Prefer minimal, targeted diffs over rewriting large files wholesale.
- If the codebase is large, use the `explore` or `general` subagent to
  parallelize the search phase (e.g. one agent per module/crate) before
  synthesizing findings yourself.
