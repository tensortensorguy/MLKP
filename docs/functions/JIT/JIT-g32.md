# JIT-g32

`predict`

## Tier 1: Beginner

`predict` predicts the posterior mean and variance at one candidate.

Inputs, outputs, and errors: none; variance is clamped at zero, all outputs finite for finite inputs.

Context: Expected Improvement needs both per-point terms; the variance is the kernel's self-covariance minus the explained part, clamped at zero.

## Tier 2: Silicon Reality

Declared cost: O(n) for the k* vector plus one triangular solve.

Declared preconditions: the model was fitted by fit().

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 148, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the model was fitted by fit()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; variance is clamped at zero, all outputs finite for finite inputs..
