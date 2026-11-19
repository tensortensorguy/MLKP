# CS-b20

`verify`

## Tier 1: Beginner

`verify` verifies an Ed25519 signature.

Inputs, outputs, and errors: Returns false on any malformed input or failed check; never panics.

Context: The linter must reject tampered or missing hunt receipts.

## Tier 2: Silicon Reality

Declared cost: cold; three scalar multiplications and one hash.

Declared preconditions: the equation checked is [S]B == R + [k]A with canonical S, consistent with this implementation's sign().

Implementation: `tools/cep_lint/src/ed25519.rs` at line 799, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the equation checked is [S]B == R + [k]A with canonical S, consistent with this implementation's sign()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns false on any malformed input or failed check; never panics..
