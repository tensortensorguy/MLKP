# JIT-s83

`airlock_rejects_bad_schedule_config`

## Tier 1: Beginner

`airlock_rejects_bad_schedule_config` verifies the airlock rejects unexecutable schedule configurations.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: A loop order or thread count outside the executable set would make the cost claims false.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 405, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
