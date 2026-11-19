# JIT-e27

`exp_sse2`

## Tier 1: Beginner

`exp_sse2` computes exp for four lanes with the SSE2 engine, the same polynomial without fused multiply-add.

Inputs, outputs, and errors: none; same saturation and NaN behavior.

Context: Non-AVX2 x86_64 hosts must run vector code too; the algorithm matches the AVX2 path operation for operation.

## Tier 2: Silicon Reality

Declared cost: about 16 vector operations per 4 lanes.

Declared preconditions: SSE2 is present on every x86_64 host.

Implementation: `crates/mlkp-runtime/src/simd/mod.rs` at line 256, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: SSE2 is present on every x86_64 host..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; same saturation and NaN behavior..
