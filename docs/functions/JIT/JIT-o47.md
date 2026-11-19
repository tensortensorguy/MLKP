# JIT-o47

`arith_program`

## Tier 1: Beginner

`arith_program` builds the arithmetic fixture.

Inputs, outputs, and errors: Panics via expect.

Context: Covers mov, add, sub, imul with caller-saved registers.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/exec_test.rs` at line 147, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect..
