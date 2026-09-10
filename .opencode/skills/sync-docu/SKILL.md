---
name: sync-docu
description: Compare the current source code against the specs/documentation and update the spec files where they no longer match the implemented behavior. Use when the user asks to "sync docs", "sync-docu", "update the spec", or wants documentation checked for drift against the code.
---

# Sync Docu

Compare the actual code behavior against the project's specification/docs
files and update the specs so they accurately reflect the code — never the
other way around (code is the source of truth for *current behavior*; specs
are the source of truth for *intended* behavior, so flag genuine
intent-vs-implementation conflicts instead of just overwriting them).

## Workflow

1. **Locate the spec/doc set.**
   - In this repo that is the `specs/` directory (e.g. `vision.md`,
     `roadmap.md`, `architecture.md`, `keyboard-modes.md`,
     `implementation-notes.md`, `app-flow.md`, `risks.md`). Also check for a
     top-level `README.md` or `AGENTS.md` if they describe behavior.
   - Read every relevant spec file fully before comparing.

2. **Map specs to code.**
   - For each claim in the specs (behavior, keybindings, module
     responsibilities, data flow, CLI flags, config options, file
     structure), find the corresponding code (use Grep/Glob or the `explore`
     agent for larger sweeps) and confirm whether it matches.
   - Pay special attention to specs describing:
     - Keyboard shortcuts / modes (`keyboard-modes.md`)
     - Architecture / module boundaries (`architecture.md`)
     - App flow / user-facing behavior (`app-flow.md`)
     - Implementation details that may have changed (`implementation-notes.md`)

3. **Classify each discrepancy found:**
   - **Doc is outdated** — code was changed/extended but the spec wasn't
     updated. → Update the spec to match the code.
   - **Code diverges from stated intent** — the spec describes a deliberate
     design decision the code no longer follows and it looks unintentional
     (e.g. a regression, a partially finished refactor). → Do **not** just
     silently update the spec to match; report this to the user and ask
     which one should win, unless it's an unambiguous case of the doc simply
     being stale/superseded.
   - **Spec describes planned but not-yet-implemented work** (e.g. items in
     `roadmap.md`). → Leave these alone; they're intentionally
     forward-looking, not drift.

4. **Update the spec files.**
   - Make precise, minimal edits — correct the specific stale sentence,
     table row, or code sample rather than rewriting whole sections.
   - Preserve the existing structure, tone, and formatting conventions of
     each file.
   - If a spec section is now entirely obsolete (feature removed), remove or
     clearly mark it rather than leaving stale claims.

5. **Summarize.**
   - Report to the user, file by file: what was out of sync, what you
     changed, and any discrepancies you intentionally left for them to
     decide on (per step 3's second category).

## Rules

- Read the actual code before changing a spec — never guess or assume based
  on the spec text alone.
- Do not invent new specs or documentation files unless the user explicitly
  asks for it; only update existing ones.
- Do not touch roadmap/backlog-style forward-looking content just because
  it isn't implemented yet.
- Keep edits scoped to genuine drift; do not do unrelated proofreading or
  rewrites of correct content.
- If unsure whether a mismatch is intentional (code) or accidental (docs
  stale), ask the user rather than guessing.
