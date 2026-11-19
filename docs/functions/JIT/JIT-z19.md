# JIT-z19

`kernels_compute_correct_values`

## Tier 1: Beginner

`kernels_compute_correct_values` verifies unary and binary kernel numerics.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The kernels are the ground truth the shadow correctness check compares against.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 17, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
