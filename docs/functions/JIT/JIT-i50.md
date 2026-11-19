# JIT-i50

`verify`

## Tier 1: Beginner

`verify` verifies one graph.

Inputs, outputs, and errors: Returns one GraphError per violation.

Context: Verifiable optimization (design section 16) is a named property of the whole design; this is its enforcement point, including the shape functions of contractions and softmax.

## Tier 2: Silicon Reality

Declared cost: O(nodes + edges).

Declared preconditions: none

Implementation: `crates/mlkp-hir/src/verify.rs` at line 25, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns one GraphError per violation..
