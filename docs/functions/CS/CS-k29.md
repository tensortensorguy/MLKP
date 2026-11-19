# CS-k29

`fe_tobytes`

## Tier 1: Beginner

`fe_tobytes` encodes a field element canonically as 32 little-endian bytes.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Point and scalar encodings are byte strings.

## Tier 2: Silicon Reality

Declared cost: cold; O(1).

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 239, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
