# JIT-n97

`executes_sse_extended`

## Tier 1: Beginner

`executes_sse_extended` tests extended SSE execution.

Inputs, outputs, and errors: Panics via assert.

Context: Multiplication, roots, division, and conversions execute together in one kernel.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/isa_exec_test.rs` at line 194, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
