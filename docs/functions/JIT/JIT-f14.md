# JIT-f14

`fusion_dimension_gates_exec_fusion`

## Tier 1: Beginner

`fusion_dimension_gates_exec_fusion` verifies the fused_elementwise schedule dimension gates exec fusion: fused and unfused execution produce bit-identical outputs.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The dimension must change real execution (candidates measure it), and the unfused path must stay exactly correct.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: lowering assigns one buffer per node in id order.

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 459, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: lowering assigns one buffer per node in id order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
