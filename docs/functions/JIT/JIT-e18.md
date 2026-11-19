# JIT-e18

`matmul_fma_matches_reference`

## Tier 1: Beginner

`matmul_fma_matches_reference` verifies the register-blocked FMA matmul against the textbook reference across ragged sizes, tile shapes, and both loop orders.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The vector kernel must compute the same values as the scalar reference on every extent, including the scalar-tail paths the ragged sizes force.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: the tolerance contract covers FMA's single-rounding accumulation.

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 104, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the tolerance contract covers FMA's single-rounding accumulation..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
