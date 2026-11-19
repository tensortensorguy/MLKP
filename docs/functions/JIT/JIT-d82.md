# JIT-d82

`emit_alu`

## Tier 1: Beginner

`emit_alu` emits integer ALU forms.

Inputs, outputs, and errors: Returns BadOperand for shapes validation already rejected.

Context: Add, Sub, And, Or, Xor, and Cmp share opcode families and shapes; one path keeps them byte-consistent.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: the instruction passed validation.

Implementation: `crates/mlkp-machine/src/encode/insn.rs` at line 443, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the instruction passed validation..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns BadOperand for shapes validation already rejected..
