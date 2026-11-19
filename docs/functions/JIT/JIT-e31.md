# JIT-e31

`matmul_fma_sched_avx2`

## Tier 1: Beginner

`matmul_fma_sched_avx2` the schedule traversal of the FMA engine: i blocks, then the schedule's outer/inner tile order over (j, k), then the register-blocked tile kernel.

Inputs and outputs are documented in the signature; the function does not fail.

Context: The autotuner's loop-order and tile dimensions must change real loop structure in the vector engine too, so candidates measure schedules, not one fixed order.

## Tier 2: Silicon Reality

Declared cost: O(m*n*k).

Declared preconditions: AVX2 and FMA enabled; extents already clamped by the caller; c_base is the row-index origin of the c view (0 for full outputs, i0 for row slices).

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 120, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: AVX2 and FMA enabled; extents already clamped by the caller; c_base is the row-index origin of the c view (0 for full outputs, i0 for row slices)..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
