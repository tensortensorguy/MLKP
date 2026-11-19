# JIT-l90

`lower_and_execute_end_to_end`

## Tier 1: Beginner

`lower_and_execute_end_to_end` lowers the canonical graph and executes it end to end.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The plan pipeline is the compilation result every candidate produces.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 99, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
