# JIT-e30

`softmax_avx2`

## Tier 1: Beginner

`softmax_avx2` the AVX2 body of stable softmax.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Max, exp, and normalize passes all run 8-wide; the exp sum accumulates in vectors.

## Tier 2: Silicon Reality

Declared cost: three vector passes per row.

Declared preconditions: AVX2 and FMA enabled; red >= 16.

Implementation: `crates/mlkp-runtime/src/simd/reduce.rs` at line 172, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; red >= 16..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
