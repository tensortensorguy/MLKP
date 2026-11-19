# JIT-e21

`exec_fusion_matches_manual_composition`

## Tier 1: Beginner

`exec_fusion_matches_manual_composition` verifies exec-level fusion end to end: the dispatcher's fused epilogue and fused chain produce bit-identical output to the manual kernel composition.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The dispatcher chooses fusions from plan structure alone; this test proves those choices compute exactly what the unfused kernel sequence computes.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: lowering assigns one buffer per node in id order.

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 216, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: lowering assigns one buffer per node in id order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
