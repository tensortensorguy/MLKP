# JIT-b96

`emit_insn`

## Tier 1: Beginner

`emit_insn` emits one GPU instruction into SPIR-V words.

Inputs, outputs, and errors: Returns EmitError::MissingResult when a value-producing instruction lacks a result id; the verifier rejects that case first, so the error is defensive.

Context: The per-opcode mapping is the emitter's core; all ids resolve through the deterministic tables and no constant instruction is emitted inside the function body.

## Tier 2: Silicon Reality

Declared cost: O(1) per instruction.

Declared preconditions: the kernel passed verify_module.

Implementation: `crates/mlkp-spirv/src/emit/insn.rs` at line 15, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the kernel passed verify_module..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns EmitError::MissingResult when a value-producing instruction lacks a result id; the verifier rejects that case first, so the error is defensive..
