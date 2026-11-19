# JIT-e08

`reduce_fast`

## Tier 1: Beginner

`reduce_fast` reduces the last axis with vector accumulators (four independent accumulator vectors per row).

Inputs, outputs, and errors: none; rows shorter than sixteen elements run the scalar kernel, where it is the fast path.

Context: The reduce kernel is memory-bound; four accumulators hide the add latency and the horizontal combine runs once per row.

## Tier 2: Silicon Reality

Declared cost: O(outer * red) at 8-wide loads.

Declared preconditions: src.len() == outer * red and dst.len() == outer; reduction order differs from the sequential scalar kernel only in association, inside the tolerance contract.

Implementation: `crates/mlkp-runtime/src/simd/reduce.rs` at line 34, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: src.len() == outer * red and dst.len() == outer; reduction order differs from the sequential scalar kernel only in association, inside the tolerance contract..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; rows shorter than sixteen elements run the scalar kernel, where it is the fast path..
