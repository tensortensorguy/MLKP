# JIT-b02

`make_program`

## Tier 1: Beginner

`make_program` generates a source program.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Parse, compile, and execute scenarios all scale on one generator so their numbers are comparable per op.

## Tier 2: Silicon Reality

Declared cost: O(ops).

Declared preconditions: the front-end accepts matmul, relu, exp, neg, sigmoid, and softmax on f32[64,64] chains.

Implementation: `crates/mlkp/benches/pipeline.rs` at line 50, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the front-end accepts matmul, relu, exp, neg, sigmoid, and softmax on f32[64,64] chains..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
