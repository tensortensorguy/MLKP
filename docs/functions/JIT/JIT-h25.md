# JIT-h25

`threaded_matmul_matches_single_thread`

## Tier 1: Beginner

`threaded_matmul_matches_single_thread` verifies the thread-chunked matmul matches the single-threaded one.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The worker dimension must change execution without changing results.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 308, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
