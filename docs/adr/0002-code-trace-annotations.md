# ADR-0002: Scope-based code trace annotations

- Status: Proposed
- Date: 2026-10-20
- Serves: CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-DEV-005, CT-TRC-006

## Context

John requires that every line of code trace back to a requirement, so an outside auditor can
verify why each line exists. A literal tag on every line would roughly double the code, hide
the logic, and invite copied tags nobody reads.

## Decision

- Tag scopes, not lines: one `@implements` tag above each function, type, or impl block covers
  its lines; `@begin`/`@end` regions cover module-level code; config files take one `@file` tag.
- The checker still accounts for every line and fails on any line outside a tagged scope.
- Commits carry a `Refs:` trailer, so `git blame` links any line to the change and requirement
  that produced it.
- Coverage of requirements is enforced only for phases that have started (`current_phase`).
- Scopes longer than 200 lines warn, to keep tags meaningful.

Full grammar: `docs/TRACEABILITY.md`.

## Consequences

- Reviewers check that tags are honest, not just present.
- The scanner depends on tree-sitter for exact item spans; languages without a grammar use
  `@begin`/`@end` regions.
- Clewtrace's own repository becomes its first test project and its best demo.
