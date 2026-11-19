# JIT-v58

`one_sweep`

## Tier 1: Beginner

`one_sweep` performs one simplification sweep over all nodes.

Inputs, outputs, and errors: Returns the number of rewrites.

Context: The fixed-point loop calls this until it reports no change.

## Tier 2: Silicon Reality

Declared cost: O(nodes).

Declared preconditions: none

Implementation: `crates/mlkp-hir/src/passes/algebraic.rs` at line 41, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns the number of rewrites..
