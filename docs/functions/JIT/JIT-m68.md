# JIT-m68

`softmax_last_axis`

## Tier 1: Beginner

`softmax_last_axis` applies row-wise stable softmax.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The composite softmax op guarantees finite outputs under the declared numerical contract (design section 6).

## Tier 2: Silicon Reality

Declared cost: two passes over the data plus one exp per element; allocation-free.

Declared preconditions: src.len() == out.len() == outer * red; red >= 1.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 182, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == out.len() == outer * red; red >= 1..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
