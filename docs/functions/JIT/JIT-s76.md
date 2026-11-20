# JIT-s76

`pgo_records_and_persists`

## Tier 1: Beginner

`pgo_records_and_persists` verifies PGO recording, persistence, reload, and drift detection.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The PGO cache is the nervous system; losing records breaks warm starts.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-autotune/tests/autotune_test.rs` at line 57, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
