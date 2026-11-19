# JIT-f60

`matmul_relu_graph`

## Tier 1: Beginner

`matmul_relu_graph` builds a small matmul-relu graph on one device.

Inputs, outputs, and errors: Panics via expect on construction.

Context: Shared fixture for most tests.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-hir/tests/hir_test.rs` at line 14, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect on construction..
