# JIT-h82

`store_shape`

## Tier 1: Beginner

`store_shape` reports store shapes.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Byte stores name the low-byte views, which the ISA only addresses with REX active for 4..7.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/encode/bytes.rs` at line 537, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
