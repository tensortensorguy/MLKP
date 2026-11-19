# JIT-e23

`vector_softmax_and_reduce_match_reference`

## Tier 1: Beginner

`vector_softmax_and_reduce_match_reference` verifies the vector softmax and reduce paths against textbook references on wide rows.

Inputs, outputs, and errors: Panics via assert on mismatch.

Context: The vector bodies for red >= 16 are new code; their normalize and sum/max semantics must match the sequential reference inside the tolerance contract.

## Tier 2: Silicon Reality

Declared cost: cold.

Declared preconditions: association differences stay inside the contract.

Implementation: `crates/mlkp-runtime/tests/simd_test.rs` at line 388, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: association differences stay inside the contract..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert on mismatch..
