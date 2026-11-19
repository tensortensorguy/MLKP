# JIT-m68

`softmax_last_axis`

## Tier 1: Beginner

`softmax_last_axis` applies row-wise stable softmax on the SIMD engine.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The composite softmax op guarantees finite outputs under the declared numerical contract (design section 6); all three passes run 8-wide.

## Tier 2: Silicon Reality

Declared cost: three passes over the data at 8-wide vectors plus one polynomial exp per element; allocation-free.

Declared preconditions: src.len() == out.len() == outer * red; red >= 1; the exp pass uses the shared polynomial, inside the tolerance contract.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 239, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == out.len() == outer * red; red >= 1; the exp pass uses the shared polynomial, inside the tolerance contract..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
