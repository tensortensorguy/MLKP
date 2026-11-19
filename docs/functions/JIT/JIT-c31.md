# JIT-c31

`matmul_tiled_sched`

## Tier 1: Beginner

`matmul_tiled_sched` computes a tiled general matrix multiply under one schedule.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The search space varies loop order (dimension B) and the B access pattern (dimension F); schedules must change the executed loop structure or the autotuner measures noise.

## Tier 2: Silicon Reality

Declared cost: O(m*n*k) multiply-adds in every schedule; access order differs by schedule.

Declared preconditions: same length and tile assumptions as matmul_tiled; loop_order is 0 or 1 and transpose_b selects the strided-B traversal.

Implementation: `crates/mlkp-runtime/src/kernels.rs` at line 321, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: same length and tile assumptions as matmul_tiled; loop_order is 0 or 1 and transpose_b selects the strided-B traversal..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
