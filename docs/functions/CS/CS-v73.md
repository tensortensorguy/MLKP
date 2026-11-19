# CS-v73

`carry_reduce`

## Tier 1: Beginner

`carry_reduce` folds five 128-bit accumulators into five 51-bit limbs modulo p.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Shared by mul and square; the top carry wraps with factor 19 because 2^255 is congruent to 19.

## Tier 2: Silicon Reality

Declared cost: cold; O(1).

Declared preconditions: inputs are the exact accumulator layout of fe_mul.

Implementation: `tools/cep_lint/src/ed25519.rs` at line 147, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: inputs are the exact accumulator layout of fe_mul..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
