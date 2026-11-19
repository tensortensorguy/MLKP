# JIT-e12

`matmul_fma_rows`

## Tier 1: Beginner

`matmul_fma_rows` runs the register-blocked FMA matmul restricted to a row range, accumulating onto the given rows.

Inputs and outputs are documented in the signature; the function does not fail.

Context: Thread chunking splits the i dimension across workers; each worker runs the same register-blocked kernel on its own rows with no synchronization.

## Tier 2: Silicon Reality

Declared cost: O((i1-i0)*n*k) with 8-wide FMAs.

Declared preconditions: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the caller owns the zeroing discipline of c_rows exactly as in the scalar rows kernel.

Implementation: `crates/mlkp-runtime/src/simd/matmul.rs` at line 88, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** no silent fallback is declared; any non-trivial path is documented under CEP:WHY.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: c_rows.len() == (i1 - i0) * n; ranges of concurrent calls are disjoint; the caller owns the zeroing discipline of c_rows exactly as in the scalar rows kernel..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** the function does not fail.
