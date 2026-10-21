---
description: Implement one task from docs/PLAN.md or one requirement, fully traced
argument-hint: <task number or requirement ID>
---

Implement $ARGUMENTS for Clewtrace, following CLAUDE.md.

1. Find the task in docs/PLAN.md (or the requirement with `grep -A3 "$ARGUMENTS" model/requirements.sysml`). Read only the files this task needs.
2. List the requirement IDs this work serves. If any needed behavior has no requirement, stop and propose one to John instead of writing code.
3. Give a plan in five bullets or fewer: files, functions, tests.
4. Write the `@verifies` tests first and run them to see them fail.
5. Implement with `@implements` tags on every new item, per docs/TRACEABILITY.md.
6. Run `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo run -p clew-cli -- check --self` (once it exists). Fix everything.
7. If a dependency was added, add it to deps.toml.
8. Commit with a `Refs:` trailer listing the IDs, tick the task in docs/PLAN.md, and note any follow-ups under it.
