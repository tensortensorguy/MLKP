# JIT-e11

`matmul_fma_sched`

## Tier 1: Beginner

`matmul_fma_sched` runs the register-blocked AVX2+FMA matmul over the schedule's tile traversal with an optional epilogue, writing full output rows.

Inputs, outputs, and errors: none; length mismatches are debug-asserted and proven by the plan airlock; ragged extents fall to scalar tails.

Context: The matmul is 93 percent of measured execution; four-row register blocking with two 8-wide FMA accumulator pairs per row turns 2.6 cycles per MAC into the FMA engine's throughput while the schedule's tile and loop-order dimensions keep their meaning.

## Tier 2: Silicon Reality

Declared cost: O(m*n*k) with 8-wide FMAs; B rows are reloaded once per four-row block, A values are broadcast per row and reduction step.

Declared preconditions: a.len() == m*k, b.len() == k*n, c.len() == m*n; c was reset to zero first (the kernel owns the full result); the contiguous-B schedule.

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 53, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a.len() == m*k, b.len() == k*n, c.len() == m*n; c was reset to zero first (the kernel owns the full result); the contiguous-B schedule..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; length mismatches are debug-asserted and proven by the plan airlock; ragged extents fall to scalar tails..
