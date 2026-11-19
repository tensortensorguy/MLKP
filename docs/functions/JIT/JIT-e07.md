# JIT-e07

`horizontal_sum_avx2`

## Tier 1: Beginner

`horizontal_sum_avx2` sums eight f32 lanes into one scalar with pairwise adds.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The reduce and softmax kernels need a cheap horizontal combine whose rounding stays inside the tolerance contract.

## Tier 2: Silicon Reality

Declared cost: four vector adds.

Declared preconditions: AVX2 enabled.

Implementation: `crates/mlkp-runtime/src/simd/reduce.rs` at line 16, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 enabled..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
