# JIT-e94

`ew_unary_avx2`

## Tier 1: Beginner

`ew_unary_avx2` the AVX2 body of the unary pass: two vectors in flight per iteration.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Eight lanes per iteration with one scalar tail loop; the interleave hides the exp latency the same way the chain pass does.

## Tier 2: Silicon Reality

Declared cost: O(n/8) vector iterations.

Declared preconditions: AVX2 and FMA enabled; lengths equal.

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 41, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; lengths equal..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
