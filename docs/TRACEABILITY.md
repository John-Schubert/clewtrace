# Code traceability rules

Implements CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-DEV-005, and CT-TRC-006.

Every non-blank, non-comment line of first-party source code must sit inside a scope tagged
with at least one valid requirement ID. `clew check --self` enforces this in CI and lists every
untraced line by file and line number.

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
4. A single tagged scope longer than `max_scope_lines` in `trace.toml` (default 200) produces a
   warning. Large scopes usually mean the tag is too coarse to be meaningful.
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
  at least one `@implements` scope and one `@verifies` test, or a `[[waiver]]` in `trace.toml`
  with a reason, the approver, and a date.
- Requirements with verification method Inspection or Demonstration may be verified by a
  checklist file in `docs/verification/` that carries an `@verifies` tag in its first comment.

## Commits

Every commit message ends with a trailer naming at least one requirement:

```
Add coverage gap report to clew check

Refs: CT-TRC-004, CT-TRC-008
```

`git blame` on any line then leads to the commit and the requirement behind it.
`clew check-commit` validates the trailer, and CI runs it on every commit in a pull request.

## What `clew check --self` reports

| Finding | Severity |
| --- | --- |
| Line outside any tagged scope | Error |
| Unknown requirement ID in a tag, trailer, `deps.toml`, or ADR | Error |
| Requirement due in a started phase with no implementation or no verification, and no waiver | Error |
| Commit without a `Refs:` trailer | Error |
| File under a source root whose type cannot be traced, and no `[[exclude]]` | Error |
| `[[exclude]]` with an empty reason or an unknown requirement | Error |
| `[[exclude]]` that matches no file | Warning |
| Tagged scope longer than `max_scope_lines` | Warning |
| Requirement with an unusually large or small code footprint | Info, in the report |

Each finding prints as `path:line: severity: message` with a `help:` line saying how to fix
it, then a one-line summary. `clew check --self` runs from the repository root and exits 0
when there are no errors, 1 when there is at least one error, and 2 when it cannot run, for
example when `trace.toml` is missing or malformed. Commit trailers are checked by
`clew check-commit`, and requirement coverage is checked from task 1.9.
