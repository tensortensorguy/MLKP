# JIT-g34

`erf`

## Tier 1: Beginner

`erf` the Abramowitz-Stegun 7.1.26 error-function approximation.

Inputs, outputs, and errors: none; monotone odd function, clamped to [-1, 1] outside the fit's |x| <= 3.5 domain.

Context: std has no erf; the rational approximation is deterministic, allocation-free, and accurate enough for acquisition ranking.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-autotune/src/gp.rs` at line 211, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; monotone odd function, clamped to [-1, 1] outside the fit's |x| <= 3.5 domain..
