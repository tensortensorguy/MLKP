# CS-l54

`compress`

## Tier 1: Beginner

`compress` compresses one 128-byte block into the chaining state.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The compression function is the core of the Merkle-Damgard construction.

## Tier 2: Silicon Reality

Declared cost: cold; O(1) per block (80 rounds).

Declared preconditions: block length is exactly 128 bytes; enforced by chunks_exact above.

Implementation: `tools/cep_lint/src/sha512.rs` at line 143, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: block length is exactly 128 bytes; enforced by chunks_exact above..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
