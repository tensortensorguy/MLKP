# JIT-f10

`run_other`

## Tier 1: Beginner

`run_other` runs one non-matmul, non-unary op (binary, reduce, softmax) over disjoint arena views.

Inputs, outputs, and errors: Returns ExecError::Buffer when a range is unavailable or overlapping.

Context: These ops keep their standalone kernels; no fusion applies to them in this plan vocabulary.

## Tier 2: Silicon Reality

Declared cost: O(kernel work).

Declared preconditions: the airlock proved disjointness.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 439, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the airlock proved disjointness..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns ExecError::Buffer when a range is unavailable or overlapping..
