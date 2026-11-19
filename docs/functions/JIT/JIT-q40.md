# JIT-q40

`executes_spilled_program`

## Tier 1: Beginner

`executes_spilled_program` tests spill execution.

Inputs, outputs, and errors: Panics via assert.

Context: Slot displacements, the frame size, and read-modify-write preloads all execute together here.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: the allocator spills computed (non-constant) values to slots.

Implementation: `crates/mlkp-machine/tests/exec_test.rs` at line 349, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the allocator spills computed (non-constant) values to slots..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
