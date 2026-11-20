# JIT-g37

`gp_fits_and_predicts_held_out_points`

## Tier 1: Beginner

`gp_fits_and_predicts_held_out_points` verifies the GP surrogate interpolates a smooth held-out function and orders uncertainty by proximity to the training set.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The Phase 2 acquisition is only as good as its posterior; held-out predictions prove the Cholesky solve, normalization, and kernel agree, and the variance ordering proves the uncertainty term is real.

## Tier 2: Silicon Reality

Declared cost: cold; one fit and three predictions.

Declared preconditions: the RBF kernel with a median-heuristic lengthscale interpolates a smooth quadratic within the stated tolerance.

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 316, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the RBF kernel with a median-heuristic lengthscale interpolates a smooth quadratic within the stated tolerance..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
