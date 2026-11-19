# JIT-e05

`ew_unary_chain_fast`

## Tier 1: Beginner

`ew_unary_chain_fast` runs a fused unary chain (up to four ops composed per element) as one pass over the buffers.

Inputs, outputs, and errors: none; empty chains are rejected by debug assertion.

Context: Chains of elementwise ops pay one load and one store instead of one per op; the composition applies in registers, so results are bit-identical to the unfused sequence.

## Tier 2: Silicon Reality

Declared cost: O(n) total instead of O(kinds * n) memory traffic.

Declared preconditions: a.len() == out.len(); kinds is non-empty (the dispatcher guarantees 1..=4).

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 102, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a.len() == out.len(); kinds is non-empty (the dispatcher guarantees 1..=4)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; empty chains are rejected by debug assertion..
