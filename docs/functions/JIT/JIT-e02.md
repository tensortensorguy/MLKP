# JIT-e02

`exp_scalar`

## Tier 1: Beginner

`exp_scalar` computes exp via the shared minimax polynomial: exponent split, two-part ln2 correction, degree-6 Horner, and an exponent-field scale.

Inputs, outputs, and errors: none; saturates to inf above the overflow threshold and to zero below the underflow threshold, propagates NaN.

Context: Every exp on every path (vector body, scalar tail, non-x86_64 hosts) uses this one algorithm so the whole stack shares one numerical contract; the polynomial's relative error is below 1.3e-7, far inside the plan's absolute tolerance.

## Tier 2: Silicon Reality

Declared cost: about 12 f32 operations per element, no calls.

Declared preconditions: outputs below 2^-126 flush to zero (absolute error below 1e-37); the magic-number rounding matches the vector path's round-to-nearest-even.

Implementation: `crates/mlkp-runtime/src/simd/mod.rs` at line 119, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: outputs below 2^-126 flush to zero (absolute error below 1e-37); the magic-number rounding matches the vector path's round-to-nearest-even..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; saturates to inf above the overflow threshold and to zero below the underflow threshold, propagates NaN..
