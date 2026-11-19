# JIT-e20

`unary_chain_is_bit_exact`

## Tier 1: Beginner

`unary_chain_is_bit_exact` verifies the fused unary chain is bit-identical to the sequential unary passes for every composition depth 1..=4.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: Chain fusion composes in registers what the unfused path does through memory; the f32 round trip is exact, so the results must match bit for bit.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 186, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
