# JIT-e29

`reduce_avx2`

## Tier 1: Beginner

`reduce_avx2` the vector body of the last-axis reduce.

Inputs and outputs are documented in the signature; the function does not fail.

Context: One code path serves AVX2 and SSE2-capable hosts; SSE2 hosts take the same body with 8-wide AVX2 loads only when AVX2 exists, so the body is AVX2-only and SSE2 hosts fall to the scalar kernel (rows shorter than the vector width dominate small graphs anyway).

## Tier 2: Silicon Reality

Declared cost: four accumulator vectors per row.

Declared preconditions: AVX2 enabled; red >= 16.

Implementation: `crates/mlkp-runtime/src/simd/reduce.rs` at line 69, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 enabled; red >= 16..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
