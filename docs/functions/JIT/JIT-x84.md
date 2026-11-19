# JIT-x84

`assembles_gas_differential`

## Tier 1: Beginner

`assembles_gas_differential` runs the gas differential.

Inputs, outputs, and errors: Panics via assert.

Context: GNU as is the independent oracle for every form choice.

## Tier 2: Silicon Reality

Declared cost: cold; one assembler invocation.

Declared preconditions: GNU as is installed and the platform is x86-64.

Implementation: `crates/mlkp-machine/tests/isa_tier_test.rs` at line 467, classification unclassified.

## Silent Failure Checklist

- **Fallbacks:** failure behavior is declared under CEP:FAILURE.
- **Truncation:** no narrowing is declared for this function; the declared assumptions state: GNU as is installed and the platform is x86-64..
- **Wrapping:** arithmetic behavior is covered by the declared failure and assumption fields; nothing wraps silently where the contract says it must not.
- **Error codes:** declared failure surface: Panics via assert..
