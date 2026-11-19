# CS-n25

`sign`

## Tier 1: Beginner

`sign` signs one message with a 32-byte seed.

Inputs, outputs, and errors: Panics if the seed is not exactly 32 bytes; the type system cannot carry the length, so the check is explicit.

Context: The CI runner signs hunt receipts so the linter can verify them without trusting the JSON text.

## Tier 2: Silicon Reality

Declared cost: cold; two scalar multiplications and three hashes.

Declared preconditions: the seed is a high-entropy secret held by the CI runner.

Implementation: `tools/cep_lint/src/ed25519.rs` at line 735, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the seed is a high-entropy secret held by the CI runner..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics if the seed is not exactly 32 bytes; the type system cannot carry the length, so the check is explicit..
