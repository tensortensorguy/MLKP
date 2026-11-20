# JIT-g35

`cholesky_packed`

## Tier 1: Beginner

`cholesky_packed` in-place Cholesky factorization of a packed symmetric positive-definite matrix.

Inputs, outputs, and errors: Returns false when a diagonal pivot is non-positive (non-PD input); the input is then unspecified.

Context: The GP solve needs one factorization reused for all per-candidate predictions; packed storage keeps the fit allocation to one buffer.

## Tier 2: Silicon Reality

Declared cost: O(n^3).

Declared preconditions: a holds the lower triangle row-major, row i at i*(i+1)/2.

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 233, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a holds the lower triangle row-major, row i at i*(i+1)/2..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns false when a diagonal pivot is non-positive (non-PD input); the input is then unspecified..
