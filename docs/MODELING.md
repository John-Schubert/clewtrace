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
  and are not nested in one;
- requirements without an implementation: no code implements them and no element satisfies
  them;
- requirements without a verification: no test or element verifies them;
- elements traced to no requirement: definitions and usages with no link, direct or through
  other links, to a requirement. An element also counts as traced when an element that owns
  it, or one it owns, is linked. Packages, metadata definitions, and the features of a
  requirement are not listed.

`clew impact <element>` takes a requirement ID such as `CT-TRC-005` or a qualified name
such as `Design::Engine`. It lists every element and code scope reachable upstream (what the
element traces to) and downstream (what traces to it), each with the link that reached it
and how many links away it is.

Both print any problems in the files first, such as syntax errors, and then exit 1. They
exit 2 when they cannot run, for example when `trace.toml` is missing.
