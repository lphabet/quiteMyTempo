---
description: Review the codebase for simplification opportunities, duplicated code, and best-practice violations, then refactor where sensible.
agent: build
---

Use the code-review-refactor skill to review the codebase: $ARGUMENTS

Check for:
- Code that can be simplified
- Duplicated code across files/modules
- Dead code and unused dependencies
- Unsafe error handling (unwrap/expect/panic in non-test code)
- Naming and module-structure consistency with best practices
- Missing test coverage on critical paths (report only, don't write tests unless asked)

Apply refactorings where they are low-risk and clearly beneficial. For
anything risky (API/behavior changes), summarize and ask before applying.
Follow AGENTS.md quality gates before committing.
