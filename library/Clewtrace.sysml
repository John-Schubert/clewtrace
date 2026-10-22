/*
 * Clewtrace library: the metadata that marks Clewtrace trace links.
 *
 * Clewtrace builds this package in, so Clewtrace models need no import. Other SysML v2
 * tools load this file and add `private import Clewtrace::*;` to each package that uses
 * `#derivation` or `#evidence`. `#refinement` comes from the standard ModelingMetadata
 * library, so those tools also need `private import ModelingMetadata::*;` for it.
 * Link syntax and meaning: docs/MODELING.md.
 */
package Clewtrace {
    doc /* Metadata for the Clewtrace trace links `#derivation` and `#evidence`. */

    metadata def <derivation> Derivation {
        doc /* Marks `#derivation dependency a to b;`: a is derived from b. */
    }

    metadata def <evidence> Evidence {
        doc /* Marks `#evidence dependency a to b;`: a is evidence for b. */
    }
}
