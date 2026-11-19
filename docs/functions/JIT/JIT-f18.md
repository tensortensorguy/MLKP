# JIT-f18

`fused_touches`

## Tier 1: Beginner

`fused_touches` groups the plan's ops exactly as the dispatcher executes them and returns per-step touches.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Liveness for the buffer-reuse pass must reflect the fused execution (skipped intermediate writes, epilogue writes at the matmul's op index); keeping the grouping next to the dispatcher's own helpers makes the two cannot drift.

## Tier 2: Silicon Reality

Declared cost: O(plan size).

Declared preconditions: the dispatcher's grouping logic (chain_extent, matmul_epilogue) is the semantics this function mirrors.

Implementation: `crates/mlkp-runtime/src/exec.rs` at line 132, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: the dispatcher's grouping logic (chain_extent, matmul_epilogue) is the semantics this function mirrors..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
