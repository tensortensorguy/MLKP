# CS-w55

`field_arithmetic_identities`

## Tier 1: Beginner

`field_arithmetic_identities` verifies field distributivity and inversion.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Multiplication and inversion bugs break all group formulas.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 883, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
