# JIT-y52

`driver_autotunes_end_to_end`

## Tier 1: Beginner

`driver_autotunes_end_to_end` runs the full driver end to end and verifies the winner beats or matches the baseline.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The 19-step lifecycle must produce a valid, verified, cached plan.

## Tier 2: Silicon Reality

Declared cost: cold but measures real executions; bounded by a short budget.

Declared preconditions: none

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 235, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
