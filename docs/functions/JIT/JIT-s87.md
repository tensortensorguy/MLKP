# JIT-s87

`control_kernel`

## Tier 1: Beginner

`control_kernel` builds a kernel with a workgroup barrier and a conditional branch.

Inputs, outputs, and errors: Panics via expect.

Context: The barrier lowering and the branch lowering must pass the word verifier like the straight-line kernel does.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-spirv/tests/spirv_test.rs` at line 200, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect..
