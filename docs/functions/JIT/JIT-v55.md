# JIT-v55

`parses_matmul_relu`

## Tier 1: Beginner

`parses_matmul_relu` parses the canonical matmul-relu program end to end.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The happy path guards signature, statement, and return parsing at once.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-front/tests/front_test.rs` at line 15, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
