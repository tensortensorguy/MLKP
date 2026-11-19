# JIT-f19

`chain_graph_plan_full`

## Tier 1: Beginner

`chain_graph_plan_full` lowers the matmul + relu + sigmoid + exp + neg chain graph under one flag set.

Inputs, outputs, and errors: Panics via expect on lowering.

Context: Shared fixture for the fusion-dimension and buffer-reuse tests.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-runtime/tests/runtime_test.rs` at line 547, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via expect on lowering..
