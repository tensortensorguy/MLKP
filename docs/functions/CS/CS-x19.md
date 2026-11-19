# CS-x19

`scalar_mul_mod_l`

## Tier 1: Beginner

`scalar_mul_mod_l` multiplies two scalars modulo L.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The signing equation needs k*s mod L.

## Tier 2: Silicon Reality

Declared cost: cold; O(256) iterations of double, conditional subtract, add, conditional subtract.

Declared preconditions: a may use the full 255 bits (the clamped signing scalar is not reduced) and is normalized modulo L on entry; b may use the full 255 bits; the accumulator stays below 2^254 between conditional subtractions.

Implementation: `tools/cep_lint/src/ed25519.rs` at line 662, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a may use the full 255 bits (the clamped signing scalar is not reduced) and is normalized modulo L on entry; b may use the full 255 bits; the accumulator stays below 2^254 between conditional subtractions..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
