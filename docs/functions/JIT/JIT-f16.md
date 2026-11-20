# JIT-f16

`space_covers_fusion_and_reuse_dimensions`

## Tier 1: Beginner

`space_covers_fusion_and_reuse_dimensions` verifies the search space covers both values of the fusion and buffer-reuse dimensions.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: A stuck dimension would measure noise; both sides of each dimension must reach the evaluator.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 289, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
