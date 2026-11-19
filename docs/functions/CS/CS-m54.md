# CS-m54

`scalar_reduce_wide`

## Tier 1: Beginner

`scalar_reduce_wide` reduces a 512-bit little-endian value modulo L by bit-wise division.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Nonces and challenge scalars are SHA-512 outputs that must land in [0, L); the doubling carry propagates from low limbs to high limbs because bits shift upward.

## Tier 2: Silicon Reality

Declared cost: cold; O(512) conditional subtractions.

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 575, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
