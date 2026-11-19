# JIT-f15

`buffer_reuse_pass_compacts_arena`

## Tier 1: Beginner

`buffer_reuse_pass_compacts_arena` verifies the buffer-reuse pass: the reused plan compacts the arena below the lens sum, passes the airlock, and executes bit-identically to the prefix-sum plan.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The memory objective rewards the reuse pass only if reuse keeps values exact and the footprint metric reflects the real arena.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: lowering assigns one buffer per node in id order.

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 498, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: lowering assigns one buffer per node in id order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
