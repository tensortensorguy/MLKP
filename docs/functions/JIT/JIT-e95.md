# JIT-e95

`ew_chain_avx2`

## Tier 1: Beginner

`ew_chain_avx2` the AVX2 body of the fused chain pass: two vectors in flight per iteration.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The composition applies per vector, one load and one store for the whole chain; the two-vector interleave hides the exp polynomial's serial latency, which the 64-op profile showed dominates the elementwise passes.

## Tier 2: Silicon Reality

Declared cost: O(n/8) iterations, each applying kinds.len() transforms.

Declared preconditions: AVX2 and FMA enabled; lengths equal; kinds non-empty.

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 150, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; lengths equal; kinds non-empty..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
