# CS-v37

`point_decode`

## Tier 1: Beginner

`point_decode` decodes a point from 32 bytes, recovering x by square root.

Inputs, outputs, and errors: Returns None for non-canonical y, x2 not a square, or zero with a sign bit set.

Context: Verification consumes encoded keys and signature halves.

## Tier 2: Silicon Reality

Declared cost: cold; O(255) squarings for the root and inversion.

Declared preconditions: none

Implementation: `tools/cep_lint/src/ed25519.rs` at line 483, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns None for non-canonical y, x2 not a square, or zero with a sign bit set..
