# Trace links in model files

Implements CT-TRC-002, CT-MOD-002.

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
