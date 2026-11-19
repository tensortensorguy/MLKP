# JIT-j68

`encode_shift`

## Tier 1: Beginner

`encode_shift` encodes shifts.

Inputs, outputs, and errors: IsaError::Parse for counts outside u8 and non-CL register counts.

Context: Count $1 uses the implicit-one form like GNU as.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/assemble/forms.rs` at line 562, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse for counts outside u8 and non-CL register counts..
