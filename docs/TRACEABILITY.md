# Code traceability rules

Implements CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-DEV-005, CT-DEV-010, and CT-TRC-006.

Every non-blank, non-comment line of first-party source code must sit inside a scope tagged
with at least one valid requirement ID. `clew check --self` enforces this in CI and lists every
untraced line by file and line number.

Model files trace to each other with `satisfy`, `verify`, and the `#derivation`,
`#refinement`, and `#evidence` dependencies. Their syntax and the Clewtrace library are in
[MODELING.md](MODELING.md).

## Requirement IDs

- Pattern: `CT-(EFF|USE|TRC|MOD|XCH|CMP|SEC|PLT|DEV)-[0-9]{3}`, for example `CT-TRC-004`.
- Every cited ID must exist in `model/requirements.sysml`. Unknown IDs fail the build.
- Several IDs are separated by commas: `@implements CT-TRC-004, CT-EFF-007`.

## Tags

Tags live in ordinary comments in the language of the file (`//`, `#`, or `<!-- -->`).

| Tag | Placed | Covers |
| --- | --- | --- |
| `@implements IDs` | In the comment directly above an item. Rust: function, method, trait method signature, struct, enum, union, trait, impl block, const or static, type alias, `macro_rules!`, `extern` block, or inline `mod`. TypeScript: function, class, method, or exported declaration | Every line of that item, including nested items that carry no tag of their own |
| `@verifies IDs` | Directly above a test function or test block. In TypeScript a call statement such as `describe(...)` is a test block | That test; creates a verifies link |
| `@begin IDs` ... `@end` | Around module-level code that is not an item | Every line between the markers |
| `@file IDs` | In the first comment of a file type listed in `trace.toml` under `file_level_allowed` (config, CI, scripts) | The whole file |

Rules:

1. A nested item with its own tag is covered by its own tag plus its parent's tags.
2. Imports, `use`, and `mod` lines inherit the union of the tags on items in the same file.
   A file that has no tagged item fails, so imports alone never pass.
3. Blank lines and comment-only lines are exempt. In Svelte files, a line holding only a
   `<script ...>`, `</script>`, `<style ...>`, or `</style>` tag is exempt too.
4. An `@implements` scope or `@begin`/`@end` region longer than `max_scope_lines` in
   `trace.toml` (default 200) produces a warning. Large scopes usually mean the tag is too
   coarse to be meaningful. Scopes that carry only `@verifies` tags, such as test modules, are
   exempt, since each test inside has its own tag.
5. Generated or vendored code is excluded only by an `[[exclude]]` entry in `trace.toml`, with
   a reason and the requirement it serves.
6. JSON has no comments, so it cannot carry an `@file` tag. Each JSON config file under a
   source root gets an `[[exclude]]` entry with a reason and the requirement it serves.
7. Any other file under a source root whose type cannot be traced is an error.

## Examples

Rust:

```rust
// @implements CT-TRC-004
pub fn coverage_gaps(graph: &TraceGraph) -> Vec<Gap> {
    graph.requirements().filter(|r| !graph.is_implemented(r)).map(Gap::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-TRC-004
    #[test]
    fn reports_unimplemented_requirement() { /* ... */ }
}
```

TypeScript or Svelte:

```ts
// @implements CT-USE-002
export function openCommandPalette(): void { /* ... */ }
```

```svelte
<script lang="ts">
  // @begin CT-TRC-003
  import { traceMatrix } from '$lib/api';
  let rows = $state([]);
  // @end
</script>
```

YAML (CI), allowed at file level:

```yaml
# @file CT-DEV-004, CT-PLT-001
name: ci
```

## Requirement coverage

- Each requirement whose `phase` is less than or equal to `current_phase` in `trace.toml` needs
  an implementation and a verification, or a `[[waiver]]` in `trace.toml` (see below). A
  requirement with no `phase` is always due. An implementation is an `@implements` scope or
  a model element that satisfies it; a verification is an `@verifies` test, a model element
  that verifies it, or a checklist (next bullet). `clew check --self` uses the same coverage
  as `clew report coverage`.
- Each requirement needs a parent: it derives from or refines another requirement, or is
  nested in one. Requirements typed by a requirement definition listed in
  `top_level_definitions` under `[coverage]` in `trace.toml`, or by one that specializes it,
  are top level and exempt. This repo lists `ClewtraceRequirements::ProductRequirement`.
  `clew report coverage` lists the rest under "Requirements without a parent".
- Requirements with verification method Inspection or Demonstration may be verified by a
  checklist file in `docs/verification/` that carries an `@verifies` tag in its first comment.
  The first comment must be an HTML comment at the top of the `.md` file, such as
  `<!-- @verifies CT-DEV-009, CT-DEV-013 -->`. A checklist that cites a requirement with
  another verification method, such as Test, does not count as its verification.

## Waivers

A waiver records why a due requirement may lack an implementation or a verification. Only
John approves waivers. Each one is a `[[waiver]]` table in `trace.toml`:

```toml
[[waiver]]
id = "CT-EFF-001"
reason = "Why it cannot be met in this phase, and when it will be"
status = "approved"
approved_by = "John Schubert"
date = "2026-10-22"
```

- `id` is a requirement ID from the model. One waiver per requirement.
- `reason` must not be empty.
- `status` is `approved` or `proposed`. An approved waiver names its approver in
  `approved_by`. A proposed waiver has `approved_by = ""` until John approves it; John
  approves it by setting `status = "approved"` and his name, or drops it by deleting it.
- `date` is the day it was approved, or proposed, written `YYYY-MM-DD`.
- A proposed waiver keeps the gate green but is a warning on every run, so it stays
  visible until John decides.
- `clew report coverage` lists waived requirements in their own section, with the status,
  approver, date, and reason, and leaves them out of the implementation and verification
  gaps. The summary line counts them as waived, not as implemented or verified.
- A waiver for a requirement that is implemented and verified is a warning: remove it.

## Commits

Every commit message ends with a trailer naming at least one requirement:

```
Add coverage gap report to clew check

Refs: CT-TRC-004, CT-TRC-008
```

`git blame` on any line then leads to the commit and the requirement behind it.
`clew check-commit <file>` validates the trailer, and CI runs it on every commit in a pull request.
Run `sh scripts/install-hooks.sh` (or `scripts\install-hooks.ps1` on Windows) once per clone
to run it as a `commit-msg` hook. The trailer must be in the last paragraph, after a blank
line. Lines starting with `#` are comments and are ignored, as in git.

## What `clew check --self` reports

| Finding | Severity |
| --- | --- |
| Line outside any tagged scope | Error |
| Unknown requirement ID in a tag, trailer, `deps.toml`, ADR, waiver, or checklist | Error |
| ADR in `docs/adr/` that cites no requirement | Error |
| Requirement due in a started phase with no implementation or no verification, and no waiver | Error |
| `[[waiver]]` with an empty reason, a bad date or status, a missing approver, an approver on a proposed waiver, or a second waiver for one requirement | Error |
| Proposed `[[waiver]]`, not yet approved | Warning |
| `[[waiver]]` for a requirement that is implemented and verified | Warning |
| Commit without a `Refs:` trailer | Error |
| File under a source root whose type cannot be traced, and no `[[exclude]]` | Error |
| `[[exclude]]` with an empty reason or an unknown requirement | Error |
| Direct dependency in a `Cargo.toml` with no `[[dep]]` entry in `deps.toml` | Error |
| `[[exclude]]` that matches no file | Warning |
| `@implements` scope or region longer than `max_scope_lines` | Warning |
| Requirement with an unusually large or small code footprint | Info, in the report |

Each finding prints as `path:line: severity: message` with a `help:` line saying how to fix
it, then a one-line summary. `clew check --self` runs from the repository root and exits 0
when there are no errors, 1 when there is at least one error, and 2 when it cannot run, for
example when `trace.toml` is missing or malformed. Commit trailers are checked by
`clew check-commit`.
