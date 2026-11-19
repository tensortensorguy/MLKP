# JIT-k18

`validate`

## Tier 1: Beginner

`validate` validates operand shapes and classes.

Inputs, outputs, and errors: Returns BadOperand or BadClass or ImmOutOfRange for every shape outside the subset.

Context: Shapes that cannot encode must fail at resolve time so the size fixpoint and the emit pass never see them; Law 2 forbids silently encoding a guess.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: none

Implementation: `crates/mlkp-machine/src/encode/operand.rs` at line 169, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns BadOperand or BadClass or ImmOutOfRange for every shape outside the subset..
