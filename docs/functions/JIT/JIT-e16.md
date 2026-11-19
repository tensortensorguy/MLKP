# JIT-e16

`exp_poly_matches_libm_within_contract`

## Tier 1: Beginner

`exp_poly_matches_libm_within_contract` verifies the shared exp polynomial against libm expf over a dense sweep and the saturation boundaries.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The polynomial replaces libm on every path (vector body, scalar tail, epilogues); its relative error must stay orders of magnitude inside the plan's absolute tolerance, and the IEEE edge behavior (inf, zero, NaN, exact exp(0)) must match the contract.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: outputs below 2^-126 flush to zero; the relative check applies only above 1e-30, the absolute check below it.

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 15, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: outputs below 2^-126 flush to zero; the relative check applies only above 1e-30, the absolute check below it..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
