# JIT-g36

`chol_solve_packed`

## Tier 1: Beginner

`chol_solve_packed` solves L L^T x = b in place against a packed Cholesky factor.

Inputs, outputs, and errors: none; the caller guarantees a successful factorization.

Context: Both the fit (alpha) and predictions reuse the same factorization, so one triangular pair serves both paths.

## Tier 2: Silicon Reality

Declared cost: O(n^2).

Declared preconditions: chol came from a successful cholesky_packed on the same n.

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 263, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: chol came from a successful cholesky_packed on the same n..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; the caller guarantees a successful factorization..
