# CS-v19

`sha512`

## Tier 1: Beginner

`sha512` computes the SHA-512 digest of a byte message.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Ed25519 keys and signatures are derived from SHA-512.

## Tier 2: Silicon Reality

Declared cost: cold; O(n) with 128-byte compression steps.

Declared preconditions: none

Implementation: `tools/cep_lint/src/sha512.rs` at line 114, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
