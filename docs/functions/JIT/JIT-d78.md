# JIT-d78

`rewrite_spills`

## Tier 1: Beginner

`rewrite_spills` rewrites spilled vregs into folded operands, loads, stores, and rematerialized constants.

Inputs, outputs, and errors: Returns AllocError::Verify when a spilled vreg somehow lacks a slot; the allocator's own invariants make this unreachable.

Context: Spill selection without rewriting leaves values unexecutable; folding keeps the consumer's register pressure unchanged so the fixpoint converges, per-use temporaries split each spilled live range at its uses and defs (ticket CEP-323), and rematerialization drops constant-defined vregs from the frame entirely.

## Tier 2: Silicon Reality

Declared cost: O(insns * spilled) per round.

Declared preconditions: slots are addressed at fixed rbp-relative offsets below the worst-case saved-register area, which the emitter reserves identically; x86-64 accepts at most one memory operand per instruction, so at most one slot use folds per instruction.

Implementation: `crates/mlkp-machine/src/irc/rewrite.rs` at line 23, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: slots are addressed at fixed rbp-relative offsets below the worst-case saved-register area, which the emitter reserves identically; x86-64 accepts at most one memory operand per instruction, so at most one slot use folds per instruction..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Returns AllocError::Verify when a spilled vreg somehow lacks a slot; the allocator's own invariants make this unreachable..
