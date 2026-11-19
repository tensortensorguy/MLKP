# JIT-e04

`ew_unary_fast`

## Tier 1: Beginner

`ew_unary_fast` runs one unary pass over full buffers with the host's widest engine and the shared scalar tail.

Inputs, outputs, and errors: none; length mismatches are debug-asserted and proven by the plan airlock.

Context: The elementwise hot path must run vector code on every x86_64 host and the same polynomial everywhere.

## Tier 2: Silicon Reality

Declared cost: O(n) at 8 (AVX2) or 4 (SSE2) lanes plus a scalar tail.

Declared preconditions: a.len() == out.len().

Implementation: `crates/mlkp-runtime/src/simd/element.rs` at line 15, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: a.len() == out.len()..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: none; length mismatches are debug-asserted and proven by the plan airlock..
