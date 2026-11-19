# JIT-z70

`loop_program`

## Tier 1: Beginner

`loop_program` builds a loop program.

Inputs, outputs, and errors: Panics via expect.

Context: Branch fixpoint and loop execution coverage.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/encode_test.rs` at line 608, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect..
