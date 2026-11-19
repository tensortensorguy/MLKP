# JIT-e06

`ew_binary_fast`

## Tier 1: Beginner

`ew_binary_fast` runs one binary pass over three full buffers with the host's widest engine and the shared scalar tail.

Inputs, outputs, and errors: none; length mismatches are debug-asserted and proven by the plan airlock.

Context: Bias adds and pairwise ops are memory-bound; the vector body halves the instruction count per element.

## Tier 2: Silicon Reality

Declared cost: O(n) with two loads and one store per element.

Declared preconditions: a.len() == b.len() == out.len().

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 205, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a.len() == b.len() == out.len()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; length mismatches are debug-asserted and proven by the plan airlock..
