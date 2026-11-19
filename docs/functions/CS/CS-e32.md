# CS-e32

`fe_mul`

## Tier 1: Beginner

`fe_mul` multiplies two field elements and reduces modulo p.

Inputs and outputs are documented in the signature; the function does not fail.

Context: All curve arithmetic reduces to field multiplication.

## Tier 2: Silicon Reality

Declared cost: cold; O(1) with u128 intermediates.

Declared preconditions: input limbs are below 2^54; output limbs are below 2^52.

Implementation: `tools/cep_lint/src/ed25519.rs` at line 104, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: input limbs are below 2^54; output limbs are below 2^52..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
