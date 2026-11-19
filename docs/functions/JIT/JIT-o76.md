# JIT-o76

`unary_div_fixture`

## Tier 1: Beginner

`unary_div_fixture` tests unary and division encodings.

Inputs, outputs, and errors: Panics via assert.

Context: The F7 group and cqo are new shapes with fixed implicit registers.

## Tier 2: Silicon Reality

Declared cost: cold

Declared preconditions: none

Implementation: `crates/mlkp-machine/tests/isa_test.rs` at line 71, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: none.
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
