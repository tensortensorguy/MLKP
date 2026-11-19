# JIT-e28

`ew_chain_sse2`

## Tier 1: Beginner

`ew_chain_sse2` the SSE2 body of the fused chain pass.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Non-AVX2 hosts compose per four lanes with the same polynomial.

## Tier 2: Silicon Reality

Declared cost: O(n/4) iterations.

Declared preconditions: SSE2 enabled; lengths equal; kinds non-empty.

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 160, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: SSE2 enabled; lengths equal; kinds non-empty..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
