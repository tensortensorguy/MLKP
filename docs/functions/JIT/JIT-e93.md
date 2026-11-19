# JIT-e93

`max_program`

## Tier 1: Beginner

`max_program` builds the SSE compare fixture.

Inputs, outputs, and errors: Panics via expect.

Context: The SSE compare sets the same flags the integer branches consume; the pairing must execute.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/common/mod.rs` at line 340, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect..
