# JIT-e32

`fma_tile`

## Tier 1: Beginner

`fma_tile` runs the register-blocked kernel over one tile: four-row blocks with two 8-wide FMA accumulator pairs, then single rows, then scalar tails per ragged extent.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Four rows amortize the B loads across four FMA chains; accumulators start from c so multi-k-tile schedules accumulate correctly.

## Tier 2: Silicon Reality

Declared cost: 8 FMAs per reduction step per four-row block plus one B pair load.

Declared preconditions: AVX2 and FMA enabled; extents within the problem; c row base offset shifts the store index for row-slice views.

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 181, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; extents within the problem; c row base offset shifts the store index for row-slice views..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
