# JIT-f06

`matmul_tiled_sched_epi`

## Tier 1: Beginner

`matmul_tiled_sched_epi` computes one tiled multiply schedule, applying the epilogue unary to each output element as it is stored.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Matmul epilogue fusion (the exec-level lookahead) needs the consumer's unary inside the kernel's stores; with an epilogue the contiguous branch accumulates per element in the j-inner form, which is bit-identical in summation order to the k-outer form but lets the epilogue apply once per final element.

## Tier 2: Silicon Reality

Declared cost: O(m*n*k) multiply-adds in every schedule; the FMA engine serves the contiguous-B schedule, the blocked scalar traversal the rest.

Declared preconditions: same length and tile assumptions as matmul_tiled; the epilogue changes only the final store of each element, never the reduction order.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 346, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: same length and tile assumptions as matmul_tiled; the epilogue changes only the final store of each element, never the reduction order..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
