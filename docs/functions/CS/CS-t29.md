# CS-t29

`scalar_add_mod_l`

## Tier 1: Beginner

`scalar_add_mod_l` adds two scalars modulo L by adding and conditional subtracting.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The signature scalar S = r + k*s must be canonical.

## Tier 2: Silicon Reality

Declared cost: cold; O(1).

Declared preconditions: inputs are below L.

Implementation: `tools/cep_lint/src/ed25519.rs` at line 639, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: inputs are below L..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
