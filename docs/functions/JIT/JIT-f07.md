# JIT-f07

`matmul_tiled_rows_epi`

## Tier 1: Beginner

`matmul_tiled_rows_epi` computes the tiled multiply restricted to a row range, applying the epilogue unary to each stored element.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The threaded fast path runs the same register-blocked kernel per worker with the same epilogue contract as the single-threaded fused matmul.

## Tier 2: Silicon Reality

Declared cost: O((i1-i0)*n*k) multiply-adds.

Declared preconditions: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the epilogue changes only final stores, never the reduction order.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 486, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the epilogue changes only final stores, never the reduction order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
