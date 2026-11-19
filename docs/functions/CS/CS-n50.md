# CS-n50

`fe_carry`

## Tier 1: Beginner

`fe_carry` fully carries the limb chain so each limb is below 2^51.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Canonical encoding and comparisons need reduced limbs.

## Tier 2: Silicon Reality

Declared cost: cold; O(1).

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 215, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
