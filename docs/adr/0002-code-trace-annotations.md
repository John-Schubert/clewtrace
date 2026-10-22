# ADR-0002: Scope-based code trace annotations

- Status: Accepted (John, 2026-10-21)
- Date: 2026-10-20
- Updated: 2026-10-21 (task 0.10, reviewed against what was built in phase 0)
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
- Scopes longer than `max_scope_lines` in `trace.toml` (200) warn, to keep tags meaningful.
  Scopes that carry only `@verifies` tags, such as test modules, are exempt.

Full grammar: `docs/TRACEABILITY.md`.

## As built in phase 0

- `clew check --self` (task 0.4) scans the source roots in `trace.toml` and fails on any
  untraced line, unknown ID, untraceable file type, or direct dependency missing from
  `deps.toml`. It also checks the IDs cited in `deps.toml` and in these ADRs.
- Rust and TypeScript use tree-sitter for item spans (task 0.3). Svelte files use
  `@begin`/`@end` regions only in v0; a line holding only a `<script>` or `<style>` tag
  is exempt.
- Config, CI, and script files (`*.toml`, `*.yml`, `*.yaml`, `*.sh`, `*.ps1`) take one
  `@file` tag. JSON cannot carry a comment, so each JSON config file under a source root
  needs an `[[exclude]]` entry with a reason and a requirement. Generated code is excluded
  the same way.
- `clew check-commit` (task 0.5) checks the `Refs:` trailer. The trailer key is
  case-sensitive and must be in the last paragraph; a `Refs:` in the summary line does not
  count (John, 2026-10-21). CI checks every commit in a pull request, and
  `scripts/install-hooks.sh` or `scripts/install-hooks.ps1` installs it as a `commit-msg`
  hook.
- Inspection and Demonstration requirements may be verified by a checklist in
  `docs/verification/` with an `@verifies` tag in its first comment.
- The phase-aware requirement coverage check is on since task 1.9: `clew check --self`
  fails on a requirement due in a started phase that has no implementation or no
  verification and no waiver. The rules, including Markdown documents that implement,
  workflow steps that verify, and waivers, are in `docs/TRACEABILITY.md`.

## Consequences

- Reviewers check that tags are honest, not just present.
- The scanner depends on tree-sitter for exact item spans; languages without a grammar use
  `@begin`/`@end` regions.
- Clewtrace's own repository becomes its first test project and its best demo.
