# JIT-u87

`jcc_family_fixture`

## Tier 1: Beginner

`jcc_family_fixture` tests the Jcc family.

Inputs, outputs, and errors: Panics via assert.

Context: All sixteen conditions must map to the right opcode bytes.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/isa_test.rs` at line 392, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
