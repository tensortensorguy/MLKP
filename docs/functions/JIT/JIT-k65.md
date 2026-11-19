# JIT-k65

`run`

## Tier 1: Beginner

`run` merges identical computations.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Two identical subtrees evaluate identically; merging halves kernel work.

## Tier 2: Silicon Reality

Declared cost: O(nodes^2).

Declared preconditions: nodes with distinct device placements are never merged, because device placement is semantic in this IR; distinct provenance names are never merged either, preserving information per design section 11.

Implementation: `crates/mlkp-hir/src/passes/cse.rs` at line 16, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: nodes with distinct device placements are never merged, because device placement is semantic in this IR; distinct provenance names are never merged either, preserving information per design section 11..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
