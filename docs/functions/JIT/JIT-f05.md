# JIT-f05

`ew_unary_chain`

## Tier 1: Beginner

`ew_unary_chain` runs a fused unary chain over full buffers.

Inputs, outputs, and errors: none; empty chains are rejected by debug assertion (the dispatcher guarantees 1..=4).

Context: Chains of elementwise ops pay one load and one store instead of one per op; the composition applies in registers, so results are bit-identical to the unfused sequence.

## Tier 2: Silicon Reality

Declared cost: O(n) total instead of O(kinds * n) memory traffic; allocation-free.

Declared preconditions: a.len() == out.len().

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 110, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a.len() == out.len()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; empty chains are rejected by debug assertion (the dispatcher guarantees 1..=4)..
