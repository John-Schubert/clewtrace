# Trace links and reports

Implements CT-TRC-002, CT-MOD-002, CT-TRC-004, and CT-TRC-005.

Clewtrace reads SysML v2 text. Besides `satisfy` and `verify`, a model states three kinds of
trace link with a marked dependency:

| Write | Meaning | Link kind |
| --- | --- | --- |
| `#derivation dependency a to b;` | a is derived from b | derives |
| `#refinement dependency a to b;` | a refines b | refines |
| `#evidence dependency a to b;` | a is evidence for b | evidences |
| `satisfy R by x;` | x satisfies R (with no `by`, the owner of the `satisfy` does) | satisfies |
| `verify R;` | the owner of the `verify` verifies R | verifies |

The client, a, comes before `to`, and the supplier, b, after it. Both sides take a comma
list, and each client links to each supplier. A dependency can have a short name and a name:
`#refinement dependency <'L1'> engineRefines from Parts::Engine to Reqs::speed;`.
A plain `dependency` with no mark, or any other mark, is a syntax error.

Code adds implements and verifies links through `@implements` and `@verifies` tags, as
described in [TRACEABILITY.md](TRACEABILITY.md).

## The Clewtrace library

The marks are metadata. `#refinement` is `Refinement` from the standard SysML v2
`ModelingMetadata` library. `#derivation` and `#evidence` are defined by the Clewtrace
library, `Clewtrace::Derivation` and `Clewtrace::Evidence`.

Clewtrace builds the library in, so models need no import for Clewtrace itself. For other
SysML v2 tools, the library is published as [`library/Clewtrace.sysml`](../library/Clewtrace.sysml).
Load that file in the tool, and in each package that uses the marks add:

```sysml
private import Clewtrace::*;
private import ModelingMetadata::*;
```

The standard `RequirementDerivation` library also defines a metadata with the short name
`derivation`, for derivation connections. Do not import both `RequirementDerivation::*` and
`Clewtrace::*` into the same package, or `#derivation` is ambiguous in other tools.

## Coverage and impact

Run these from the project root, where `trace.toml` lives. Both read every `.sysml` file
under the root and the tags in the code under the source roots in `trace.toml`.

`clew report coverage` lists:

- requirements without a parent: they neither derive from nor refine another requirement,
  are not nested in one, and are not top level (see below);
- requirements without an implementation: no code implements them and no element satisfies
  them;
- requirements without a verification: no test or element verifies them;
- elements traced to no requirement: definitions and usages with no link, direct or through
  other links, to a requirement. An element also counts as traced when an element that owns
  it, or one it owns, is linked. Packages, metadata definitions, and the features of a
  requirement are not listed.

Top-level requirements, such as product requirements, have no parent by design. List
their requirement definitions under `[coverage]` in `trace.toml`:

```toml
top_level_definitions = ["ClewtraceRequirements::ProductRequirement"]
```

Each entry is the qualified name of a requirement definition. A requirement typed by one of
them, or by a definition that specializes one, counts as having a parent. Every other
requirement still needs one. An entry that names no element, or names something other than a
requirement definition, stops the report with exit 2 and the `trace.toml` line. The key is
optional; without it every requirement needs a parent.

`clew impact <element>` takes a requirement ID such as `CT-TRC-005` or a qualified name
such as `Design::Engine`. It lists every element and code scope reachable upstream (what the
element traces to) and downstream (what traces to it), each with the link that reached it
and how many links away it is.

Both print a header first (see below), then any problems in the files, such as syntax
errors, and then exit 1. They exit 2 when they cannot run, for example when `trace.toml` is
missing.

## Reproducible reports

Every report from `clew report coverage` and `clew impact` starts with four lines and a
blank line:

```
Report: clew report coverage
Commit: 5d94dee0c1f3a2b4e6d8f0a1b3c5d7e9f1a3b5c7
Tool: clew 0.1.0
Inputs: sha256 9f2c...e41a over 42 files
```

- `Report` is the command that made it.
- `Commit` is the Git commit of the project, followed by `, with uncommitted changes` when
  the work tree differs from it, untracked files included. It is `none` outside a Git
  repository or when Git is not installed. clew runs only `git rev-parse HEAD` and
  `git status`, with the file system monitor turned off, so a project's Git config cannot
  make it run a hook command. Git can still run clean filters that a repository's own
  config defines, so verify reports only in projects whose `.git/config` you trust. Add
  `.clewtrace/` to `.gitignore` so the index does not count as a change.
- `Tool` is the clew version, which also fixes the built-in libraries.
- `Inputs` is a SHA-256 over every file the report read as text: `trace.toml`, the `.sysml`
  files, and the code files scanned for tags. The hash is taken over a manifest with one
  line per file, sorted by path, in the form `sha256sum` prints: the SHA-256 of the file's
  text in lowercase hex, two spaces, and its path relative to the project root with `/`
  separators. The text is hashed without a leading byte order mark and with Windows line
  endings as `\n`, so a report made on Linux verifies on a Windows checkout of the same
  commit.

Save a report to a file, for example `clew report coverage > coverage.txt`. To check it
later, run `clew report verify coverage.txt` from the same project root. It makes the
report again from the files there and compares the tool version, the inputs hash, and every
line after the header. A different commit with the same inputs is only a note. A byte order
mark and Windows line endings in the saved file are ignored. It exits 0 when the report
matches, 1 when it differs, with each difference and a `help:` line, and 2 when it cannot
compare, for example when the file is not a clew report or `trace.toml` is missing.

To reproduce a report from another machine, check out the recorded commit, install the
recorded clew version, and run `clew report verify`.
