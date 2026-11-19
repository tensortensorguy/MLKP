# JIT-s73

`push_branch`

## Tier 1: Beginner

`push_branch` encodes branch forms.

Inputs, outputs, and errors: Panics only on a non-branch opcode, which the caller dispatches away.

Context: Jmp, Jl, Jge, and the Ret jump share the displacement logic.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: disp fits the form (checked by branch_disp).

Implementation: `crates/mlkp-machine/src/encode/insn.rs` at line 585, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: disp fits the form (checked by branch_disp)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics only on a non-branch opcode, which the caller dispatches away..
