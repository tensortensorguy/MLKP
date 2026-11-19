# JIT-e22

`exec_refuses_fusion_on_fanout`

## Tier 1: Beginner

`exec_refuses_fusion_on_fanout` verifies the dispatcher refuses fusion when a matmul output feeds two consumers, and the unfused execution still produces exact values.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The read scan must never skip a buffer write that a later op observes; the fan-out graph is the case where fusion would be wrong.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: lowering assigns one buffer per node in id order.

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 308, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: lowering assigns one buffer per node in id order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
