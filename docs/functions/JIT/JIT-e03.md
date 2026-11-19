# JIT-e03

`exp_avx2`

## Tier 1: Beginner

`exp_avx2` computes exp for eight lanes with the AVX2 engine: FMA Horner over the shared polynomial and an exponent-field scale, with saturation and NaN masks.

Inputs, outputs, and errors: none; k beyond the exponent range saturates through the comparison masks, NaN input propagates.

Context: The vector body must be the same algorithm as the scalar path so fused epilogues and tails agree bit-for-bit within rounding.

## Tier 2: Silicon Reality

Declared cost: about 16 vector operations per 8 lanes.

Declared preconditions: called only with AVX2 and FMA enabled; round-to-nearest-even conversion is the default MXCSR mode.

Implementation: `crates/mlkp-runtime/src/simd/mod.rs` at line 208, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: called only with AVX2 and FMA enabled; round-to-nearest-even conversion is the default MXCSR mode..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; k beyond the exponent range saturates through the comparison masks, NaN input propagates..
