# JIT-e19

`matmul_epilogue_is_bit_exact`

## Tier 1: Beginner

`matmul_epilogue_is_bit_exact` verifies the fused matmul epilogue is bit-identical to the unfused matmul followed by the unary pass.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Epilogue fusion must never change a single output bit; the epilogue applies the same transform to the same accumulator values.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 153, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
