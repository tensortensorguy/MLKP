# JIT-e39

`encode_movsd`

## Tier 1: Beginner

`encode_movsd` encodes movsd.

Inputs, outputs, and errors: IsaError::Parse when both or neither operand is an xmm register.

Context: movsd picks load or store by which operand is memory.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/isa/assemble/forms.rs` at line 627, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse when both or neither operand is an xmm register..
