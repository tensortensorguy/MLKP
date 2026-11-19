# JIT-j18

`encode_test`

## Tier 1: Beginner

`encode_test` encodes test.

Inputs, outputs, and errors: IsaError::Parse for shapes the tier does not encode.

Context: test is commutative; the reg field holds the register operand.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/assemble/forms.rs` at line 479, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse for shapes the tier does not encode..
