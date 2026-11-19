# JIT-i28

`modrm_tail`

## Tier 1: Beginner

`modrm_tail` emits a ModRM tail.

Inputs, outputs, and errors: IsaError::Parse for displacements outside i32.

Context: The caller owns the single REX byte; splitting keeps one REX per instruction.

## Tier 2: Silicon Reality

Declared cost: O(1).

Declared preconditions: the caller emitted prefix, REX, and opcode bytes already.

Implementation: `crates/mlkp-machine/src/isa/assemble/forms.rs` at line 101, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the caller emitted prefix, REX, and opcode bytes already..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: IsaError::Parse for displacements outside i32..
