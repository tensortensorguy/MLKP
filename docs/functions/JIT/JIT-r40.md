# JIT-r40

`imul3_fixture`

## Tier 1: Beginner

`imul3_fixture` tests imul3 encodings.

Inputs, outputs, and errors: Panics via assert.

Context: The destination sits in the reg field for this form, opposite the two-operand family.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/isa_test.rs` at line 325, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
