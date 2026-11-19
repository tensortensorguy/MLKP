# JIT-f11

`reduce_last_axis_scalar`

## Tier 1: Beginner

`reduce_last_axis_scalar` reduces the last axis, sequential scalar.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Rows too short to amortize a vector run here, where it is the fast path.

## Tier 2: Silicon Reality

Declared cost: O(outer * red); allocation-free.

Declared preconditions: src.len() == outer * red and dst.len() == outer.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 194, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == outer * red and dst.len() == outer..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
