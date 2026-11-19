# JIT-m25

`full_lifecycle_compiles_and_executes`

## Tier 1: Beginner

`full_lifecycle_compiles_and_executes` runs the full lifecycle once with a real budget.

Inputs, outputs, and errors: Panics via expect or assert on failure.

Context: Steps 1 through 17 of the design's lifecycle in one path.

## Tier 2: Silicon Reality

Declared cost: cold; bounded by a 2.5 second budget.

Declared preconditions: none

Implementation: `crates/mlkp/tests/lifecycle_test.rs` at line 18, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect or assert on failure..
