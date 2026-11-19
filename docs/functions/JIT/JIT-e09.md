# JIT-e09

`softmax_fast`

## Tier 1: Beginner

`softmax_fast` applies row-wise stable softmax with a vector max pass, the shared polynomial exp pass, and a broadcast-divide normalize pass.

Inputs, outputs, and errors: none; rows shorter than sixteen elements run the scalar kernel.

Context: Softmax is three memory passes plus one exp per element; vectorizing all three keeps the composite op off the hot-path critical list.

## Tier 2: Silicon Reality

Declared cost: three passes at 8-wide vectors plus one polynomial exp per element.

Declared preconditions: src.len() == out.len() == outer * red; red >= 1; outputs finite under the declared numerical contract.

Implementation: `crates/mlkp-runtime/src/simd/reduce.rs` at line 142, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == out.len() == outer * red; red >= 1; outputs finite under the declared numerical contract..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; rows shorter than sixteen elements run the scalar kernel..
