# JIT-f50

`executes_sse`

## Tier 1: Beginner

`executes_sse` tests SSE execution.

Inputs, outputs, and errors: Panics via assert.

Context: Scalar-double encodings and the xmm class path execute end to end.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/exec_test.rs` at line 446, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
