# JIT-u48

`reduce_last_axis`

## Tier 1: Beginner

`reduce_last_axis` reduces the last axis of a row-major tensor on the SIMD engine.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Softmax and loss paths reduce the contiguous axis, which is the cache-friendly case; four independent vector accumulators hide the add latency.

## Tier 2: Silicon Reality

Declared cost: O(outer * red) at 8-wide loads; allocation-free.

Declared preconditions: src.len() == outer * red and dst.len() == outer; enforced by debug assertion and the plan airlock; the vector body's association differs from the sequential scalar kernel only inside the tolerance contract.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 181, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == outer * red and dst.len() == outer; enforced by debug assertion and the plan airlock; the vector body's association differs from the sequential scalar kernel only inside the tolerance contract..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
