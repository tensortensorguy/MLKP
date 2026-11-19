# JIT-a35

`matmul_schedules_match_reference`

## Tier 1: Beginner

`matmul_schedules_match_reference` verifies the scheduled matmul variants compute the same values.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The search space varies loop order and the B access pattern; every schedule must be a correct matmul or the autotuner optimizes against wrong ground truth.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 260, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
