# CS-x66

`malformed_inputs_are_rejected`

## Tier 1: Beginner

`malformed_inputs_are_rejected` verifies that malformed public keys and signatures are rejected without panicking.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The linter must never crash on hostile receipt inputs.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `tools/cep_lint/tests/ed25519_test.rs` at line 113, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
